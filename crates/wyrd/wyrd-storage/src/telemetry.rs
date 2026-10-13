//! Private storage operation metrics recorded by the handle's data plane.
//!
//! Every [`crate::StorageHandle`] data-plane call opens one [`ActiveOperation`]
//! guard, so each backend operation is counted exactly once with its outcome,
//! latency, and transferred bytes, and a dropped (cancelled) call still drains
//! its active gauge.

use std::time::Instant;

use wyrd_spec::storage::StorageBackendKind;

/// Closed storage operation labels emitted by the handle.
#[derive(Clone, Copy)]
pub(crate) enum StorageOperation {
    /// Read object bytes.
    Get,
    /// Write object bytes.
    Put,
    /// Enumerate objects.
    List,
    /// Delete objects.
    Delete,
    /// Read object metadata.
    Head,
}

impl StorageOperation {
    /// Every operation, used to register idle gauges.
    const ALL: [Self; 5] = [Self::Get, Self::Put, Self::List, Self::Delete, Self::Head];

    /// Return the bounded metric label for this operation.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Put => "put",
            Self::List => "list",
            Self::Delete => "delete",
            Self::Head => "head",
        }
    }

    /// Return the byte-direction label for operations that move object bytes.
    const fn direction(self) -> Option<&'static str> {
        match self {
            Self::Get => Some("read"),
            Self::Put => Some("write"),
            Self::List | Self::Delete | Self::Head => None,
        }
    }
}

/// Return the bounded backend label.
pub(crate) const fn backend_label(backend: StorageBackendKind) -> &'static str {
    match backend {
        StorageBackendKind::Local => "local",
        StorageBackendKind::S3 => "s3",
        StorageBackendKind::Gcs => "gcs",
        StorageBackendKind::Azure => "azure",
    }
}

/// Register the idle active-operation gauges for one backend.
pub(crate) fn register(backend: &'static str) {
    for operation in StorageOperation::ALL {
        metrics::gauge!("wyrd_storage_operations_active", "backend" => backend, "operation" => operation.as_str()).set(0.0);
    }
}

/// One active-operation guard that guarantees idle cleanup on every exit.
pub(crate) struct ActiveOperation {
    /// Backend label fixed when the handle is constructed.
    backend: &'static str,
    /// Closed operation whose gauge this guard owns.
    operation: StorageOperation,
    /// Monotonic start for terminal latency.
    started: Instant,
    /// Whether a terminal outcome was recorded.
    completed: bool,
}

impl ActiveOperation {
    /// Start one storage operation and increment its active gauge.
    pub(crate) fn start(backend: &'static str, operation: StorageOperation) -> Self {
        metrics::gauge!("wyrd_storage_operations_active", "backend" => backend, "operation" => operation.as_str()).increment(1.0);
        Self {
            backend,
            operation,
            started: Instant::now(),
            completed: false,
        }
    }

    /// Record the terminal outcome and, on success, `bytes` moved by a get or
    /// put. The result passes through unchanged.
    pub(crate) fn finish<T, E>(mut self, result: Result<T, E>, bytes: u64) -> Result<T, E> {
        let outcome = if result.is_ok() { "success" } else { "failed" };
        self.record(outcome);
        if result.is_ok()
            && let Some(direction) = self.operation.direction()
        {
            metrics::counter!("wyrd_storage_bytes_total", "backend" => self.backend, "operation" => self.operation.as_str(), "direction" => direction).increment(bytes);
        }
        result
    }

    /// Record one terminal outcome series and disarm cancellation-on-drop.
    fn record(&mut self, outcome: &'static str) {
        metrics::counter!("wyrd_storage_operations_total", "backend" => self.backend, "operation" => self.operation.as_str(), "outcome" => outcome).increment(1);
        metrics::histogram!("wyrd_storage_operation_duration_seconds", "backend" => self.backend, "operation" => self.operation.as_str(), "outcome" => outcome).record(self.started.elapsed().as_secs_f64());
        self.completed = true;
    }
}

impl Drop for ActiveOperation {
    /// Drain the active gauge and record cancellation when no outcome exists.
    fn drop(&mut self) {
        if !self.completed {
            self.record("cancelled");
        }
        metrics::gauge!("wyrd_storage_operations_active", "backend" => self.backend, "operation" => self.operation.as_str()).decrement(1.0);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use metrics::{Counter, Gauge, Histogram, HistogramFn, Key, Metadata, Recorder};

    use super::{ActiveOperation, StorageOperation};

    /// Isolated counter/gauge recorder for exact storage lifecycle assertions.
    #[derive(Default)]
    pub(crate) struct TestRecorder {
        /// Counter series keyed by [`key_name`].
        counters: Mutex<HashMap<String, Arc<metrics::atomics::AtomicU64>>>,
        /// Gauge series keyed by [`key_name`].
        gauges: Mutex<HashMap<String, Arc<metrics::atomics::AtomicU64>>>,
    }

    /// Histogram sink retaining only the number of terminal observations.
    #[derive(Default)]
    struct HistogramCount(AtomicU64);

    impl HistogramFn for HistogramCount {
        /// Count one observation.
        fn record(&self, _: f64) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl Recorder for TestRecorder {
        /// Ignore descriptions.
        fn describe_counter(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        /// Ignore descriptions.
        fn describe_gauge(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        /// Ignore descriptions.
        fn describe_histogram(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }

        /// Share one atomic per exact counter series.
        fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
            let value = Arc::clone(
                self.counters
                    .lock()
                    .expect("counter registry")
                    .entry(key_name(key))
                    .or_default(),
            );
            Counter::from_arc(value)
        }

        /// Share one atomic per exact gauge series.
        fn register_gauge(&self, key: &Key, _: &Metadata<'_>) -> Gauge {
            let value = Arc::clone(
                self.gauges
                    .lock()
                    .expect("gauge registry")
                    .entry(key_name(key))
                    .or_default(),
            );
            Gauge::from_arc(value)
        }

        /// Count histogram observations without retaining values.
        fn register_histogram(&self, _: &Key, _: &Metadata<'_>) -> Histogram {
            Histogram::from_arc(Arc::new(HistogramCount::default()))
        }
    }

    impl TestRecorder {
        /// Read one exact counter series.
        pub(crate) fn counter(&self, key: &str) -> u64 {
            self.counters
                .lock()
                .expect("counter registry")
                .get(key)
                .map_or(0, |value| value.load(Ordering::Relaxed))
        }

        /// Read one exact gauge series.
        pub(crate) fn gauge(&self, key: &str) -> f64 {
            self.gauges
                .lock()
                .expect("gauge registry")
                .get(key)
                .map_or(0.0, |value| f64::from_bits(value.load(Ordering::Relaxed)))
        }
    }

    /// Render an exact stable test key independent of label insertion order.
    fn key_name(key: &Key) -> String {
        let mut labels = key
            .labels()
            .map(|label| format!("{}={}", label.key(), label.value()))
            .collect::<Vec<_>>();
        labels.sort();
        if labels.is_empty() {
            key.name().to_owned()
        } else {
            format!("{}{{{}}}", key.name(), labels.join(","))
        }
    }

    /// A successful put counts its bytes; a failed put counts none; a dropped
    /// guard records cancellation; every path drains the active gauge.
    #[test]
    fn guard_outcomes_are_exact_and_drain_the_gauge() {
        let recorder = TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);

        let ok: Result<(), ()> =
            ActiveOperation::start("local", StorageOperation::Put).finish(Ok(()), 8);
        let failed: Result<(), ()> =
            ActiveOperation::start("local", StorageOperation::Put).finish(Err(()), 8);
        drop(ActiveOperation::start("local", StorageOperation::Put));

        assert!(ok.is_ok() && failed.is_err());
        assert_eq!(
            recorder
                .counter("wyrd_storage_bytes_total{backend=local,direction=write,operation=put}"),
            8
        );
        for outcome in ["success", "failed", "cancelled"] {
            assert_eq!(
                recorder.counter(&format!(
                    "wyrd_storage_operations_total{{backend=local,operation=put,outcome={outcome}}}"
                )),
                1
            );
        }
        assert!(
            recorder
                .gauge("wyrd_storage_operations_active{backend=local,operation=put}")
                .abs()
                <= f64::EPSILON
        );
    }
}
