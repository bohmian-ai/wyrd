//! Private `OpenDAL` instrumentation applied once to the process-owned operator.

use std::fmt::Debug;
use std::time::Instant;

use opendal::raw::{
    Access, Layer, LayeredAccess, OpDelete, OpList, OpRead, OpWrite, RpDelete, RpList, RpRead,
    RpWrite, oio,
};
use opendal::{Buffer, Metadata};

/// Closed storage operation labels emitted by the shared operator boundary.
#[derive(Clone, Copy)]
enum StorageOperation {
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
}

/// One active-operation guard that guarantees idle cleanup on every exit.
struct ActiveOperation {
    /// Backend label fixed when the operator is constructed.
    backend: &'static str,
    /// Closed operation whose gauge this guard owns.
    operation: StorageOperation,
    /// Monotonic start for terminal latency.
    started: Instant,
    /// Whether a normal terminal outcome was recorded.
    completed: bool,
}

impl ActiveOperation {
    /// Start one storage operation and increment its active gauge.
    fn start(backend: &'static str, operation: StorageOperation) -> Self {
        metrics::gauge!("wyrd_storage_operations_active", "backend" => backend, "operation" => operation.as_str()).increment(1.0);
        Self {
            backend,
            operation,
            started: Instant::now(),
            completed: false,
        }
    }

    /// Record one terminal result without changing the underlying value.
    fn finish<T, E>(mut self, result: &Result<T, E>) {
        let outcome = if result.is_ok() { "success" } else { "failed" };
        metrics::counter!("wyrd_storage_operations_total", "backend" => self.backend, "operation" => self.operation.as_str(), "outcome" => outcome).increment(1);
        metrics::histogram!("wyrd_storage_operation_duration_seconds", "backend" => self.backend, "operation" => self.operation.as_str(), "outcome" => outcome).record(self.started.elapsed().as_secs_f64());
        self.completed = true;
    }

    /// Record an intentional cancellation and disarm cancellation-on-drop.
    fn finish_cancelled(mut self) {
        metrics::counter!("wyrd_storage_operations_total", "backend" => self.backend, "operation" => self.operation.as_str(), "outcome" => "cancelled").increment(1);
        metrics::histogram!("wyrd_storage_operation_duration_seconds", "backend" => self.backend, "operation" => self.operation.as_str(), "outcome" => "cancelled").record(self.started.elapsed().as_secs_f64());
        self.completed = true;
    }
}

impl Drop for ActiveOperation {
    /// Drain the active gauge and record cancellation when no terminal result exists.
    fn drop(&mut self) {
        if !self.completed {
            metrics::counter!("wyrd_storage_operations_total", "backend" => self.backend, "operation" => self.operation.as_str(), "outcome" => "cancelled").increment(1);
            metrics::histogram!("wyrd_storage_operation_duration_seconds", "backend" => self.backend, "operation" => self.operation.as_str(), "outcome" => "cancelled").record(self.started.elapsed().as_secs_f64());
        }
        metrics::gauge!("wyrd_storage_operations_active", "backend" => self.backend, "operation" => self.operation.as_str()).decrement(1.0);
    }
}

/// Private layer installed once before the storage operator can be cloned.
#[derive(Clone, Copy, Debug)]
pub(super) struct StorageTelemetryLayer {
    /// Closed backend label derived from configuration.
    backend: &'static str,
}

impl StorageTelemetryLayer {
    /// Construct the layer and register idle operation gauges.
    pub(super) fn new(backend: wyrd_spec::storage::StorageBackendKind) -> Self {
        let backend = match backend {
            wyrd_spec::storage::StorageBackendKind::Local => "local",
            wyrd_spec::storage::StorageBackendKind::S3 => "s3",
            wyrd_spec::storage::StorageBackendKind::Gcs => "gcs",
            wyrd_spec::storage::StorageBackendKind::Azure => "azure",
        };
        for operation in [
            StorageOperation::Get,
            StorageOperation::Put,
            StorageOperation::List,
            StorageOperation::Delete,
            StorageOperation::Head,
        ] {
            metrics::gauge!("wyrd_storage_operations_active", "backend" => backend, "operation" => operation.as_str()).set(0.0);
        }
        Self { backend }
    }
}

impl<A: Access> Layer<A> for StorageTelemetryLayer {
    type LayeredAccess = StorageTelemetryAccess<A>;

    /// Wrap the concrete accessor without altering its capabilities.
    fn layer(&self, inner: A) -> Self::LayeredAccess {
        StorageTelemetryAccess {
            inner,
            backend: self.backend,
        }
    }
}

/// Accessor wrapper that observes operations while forwarding unchanged results.
pub(super) struct StorageTelemetryAccess<A> {
    /// Original `OpenDAL` accessor.
    inner: A,
    /// Closed backend label.
    backend: &'static str,
}

impl<A: Debug> Debug for StorageTelemetryAccess<A> {
    /// Redact accessor internals while retaining the backend identity.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageTelemetryAccess")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

impl<A: Access> LayeredAccess for StorageTelemetryAccess<A> {
    type Inner = A;
    type Reader = StorageReader<A::Reader>;
    type Writer = StorageWriter<A::Writer>;
    type Lister = StorageLister<A::Lister>;
    type Deleter = StorageDeleter<A::Deleter>;
    type Copier = A::Copier;

    /// Borrow the wrapped accessor for default forwarding.
    fn inner(&self) -> &Self::Inner {
        &self.inner
    }

    /// Observe read creation and the exact response content length.
    ///
    /// # Errors
    ///
    /// Returns the wrapped accessor's read-construction error unchanged. Dropping
    /// a successfully constructed reader before EOF records cancellation and any
    /// bytes already returned remain counted as partial progress.
    async fn read(&self, path: &str, args: OpRead) -> opendal::Result<(RpRead, Self::Reader)> {
        let active = ActiveOperation::start(self.backend, StorageOperation::Get);
        match self.inner.read(path, args).await {
            Ok((response, reader)) => Ok((
                response,
                StorageReader {
                    inner: reader,
                    active: Some(active),
                },
            )),
            Err(error) => {
                let failed: Result<(), &opendal::Error> = Err(&error);
                active.finish(&failed);
                Err(error)
            }
        }
    }

    /// Observe writer creation; actual write bytes remain owned by `OpenDAL`'s writer.
    ///
    /// # Errors
    ///
    /// Returns the wrapped accessor's writer-construction error unchanged.
    async fn write(&self, path: &str, args: OpWrite) -> opendal::Result<(RpWrite, Self::Writer)> {
        let active = ActiveOperation::start(self.backend, StorageOperation::Put);
        match self.inner.write(path, args).await {
            Ok((response, writer)) => Ok((
                response,
                StorageWriter {
                    inner: writer,
                    active: Some(active),
                    pending_bytes: 0,
                },
            )),
            Err(error) => {
                let failed: Result<(), &opendal::Error> = Err(&error);
                active.finish(&failed);
                Err(error)
            }
        }
    }

    /// Observe list creation without exposing the listed path.
    ///
    /// # Errors
    ///
    /// Returns the wrapped accessor's list-construction error unchanged. Dropping
    /// the lister before EOF records cancellation.
    async fn list(&self, path: &str, args: OpList) -> opendal::Result<(RpList, Self::Lister)> {
        let active = ActiveOperation::start(self.backend, StorageOperation::List);
        match self.inner.list(path, args).await {
            Ok((response, lister)) => Ok((
                response,
                StorageLister {
                    inner: lister,
                    active: Some(active),
                },
            )),
            Err(error) => {
                let failed: Result<(), &opendal::Error> = Err(&error);
                active.finish(&failed);
                Err(error)
            }
        }
    }

    /// Observe deleter creation; queued paths remain excluded from labels.
    ///
    /// # Errors
    ///
    /// Returns the wrapped accessor's deleter-construction error unchanged.
    async fn delete(&self) -> opendal::Result<(RpDelete, Self::Deleter)> {
        let active = ActiveOperation::start(self.backend, StorageOperation::Delete);
        match self.inner.delete().await {
            Ok((response, deleter)) => Ok((
                response,
                StorageDeleter {
                    inner: deleter,
                    active: Some(active),
                },
            )),
            Err(error) => {
                let failed: Result<(), &opendal::Error> = Err(&error);
                active.finish(&failed);
                Err(error)
            }
        }
    }

    /// Observe metadata reads without exposing object identity.
    ///
    /// # Errors
    ///
    /// Returns the wrapped accessor's metadata error unchanged.
    async fn stat(
        &self,
        path: &str,
        args: opendal::raw::OpStat,
    ) -> opendal::Result<opendal::raw::RpStat> {
        let active = ActiveOperation::start(self.backend, StorageOperation::Head);
        let result = self.inner.stat(path, args).await;
        active.finish(&result);
        result
    }
}

/// Reader wrapper that records successful bytes and terminal EOF/error.
pub(super) struct StorageReader<R> {
    inner: R,
    active: Option<ActiveOperation>,
}

impl<R: oio::Read> oio::Read for StorageReader<R> {
    /// Forward one read and record only bytes actually returned.
    ///
    /// # Errors
    ///
    /// Returns the wrapped reader error unchanged after recording failure. Bytes
    /// returned by earlier successful polls remain truthful partial progress.
    async fn read(&mut self) -> opendal::Result<Buffer> {
        let result = self.inner.read().await;
        match &result {
            Ok(buffer) if buffer.is_empty() => { if let Some(active) = self.active.take() { active.finish(&Ok::<(), ()>(())); } }
            Ok(buffer) => metrics::counter!("wyrd_storage_bytes_total", "backend" => self.active.as_ref().map_or("local", |active| active.backend), "operation" => "get", "direction" => "read").increment(buffer.len() as u64),
            Err(_) => { if let Some(active) = self.active.take() { active.finish(&Err::<(), ()>(())); } }
        }
        result
    }
}

/// Writer wrapper that records accepted bytes and terminal close/error.
pub(super) struct StorageWriter<W> {
    inner: W,
    active: Option<ActiveOperation>,
    /// Bytes accepted by the writer but not yet durably committed by close.
    pending_bytes: u64,
}

impl<W: oio::Write> oio::Write for StorageWriter<W> {
    /// Forward one write and count bytes only after `OpenDAL` accepts them.
    ///
    /// # Errors
    ///
    /// Returns the wrapped writer error unchanged and records no bytes for the
    /// failed write call.
    async fn write(&mut self, buffer: Buffer) -> opendal::Result<()> {
        let bytes = buffer.len() as u64;
        let result = self.inner.write(buffer).await;
        if result.is_ok() {
            self.pending_bytes = self.pending_bytes.saturating_add(bytes);
        }
        if result.is_err()
            && let Some(active) = self.active.take()
        {
            active.finish(&Err::<(), ()>(()));
        }
        result
    }

    /// Abort the write and record its unchanged terminal result.
    ///
    /// # Errors
    ///
    /// Returns the wrapped writer's abort error unchanged.
    async fn abort(&mut self) -> opendal::Result<()> {
        let result = self.inner.abort().await;
        if let Some(active) = self.active.take() {
            if result.is_ok() {
                active.finish_cancelled();
            } else {
                active.finish(&result);
            }
        }
        self.pending_bytes = 0;
        result
    }

    /// Close the write and record its unchanged terminal result.
    ///
    /// # Errors
    ///
    /// Returns the wrapped writer's close error unchanged. Accepted bytes are
    /// published only when close commits the object successfully.
    async fn close(&mut self) -> opendal::Result<Metadata> {
        let result = self.inner.close().await;
        if let Some(active) = self.active.take() {
            if result.is_ok() && self.pending_bytes > 0 {
                metrics::counter!("wyrd_storage_bytes_total", "backend" => active.backend, "operation" => "put", "direction" => "write").increment(self.pending_bytes);
            }
            active.finish(&result);
        }
        self.pending_bytes = 0;
        result
    }
}

/// Lister wrapper that keeps the operation active through terminal EOF/error.
pub(super) struct StorageLister<L> {
    inner: L,
    active: Option<ActiveOperation>,
}

impl<L: oio::List> oio::List for StorageLister<L> {
    /// Forward one page and finish the lifecycle at EOF or failure.
    ///
    /// # Errors
    ///
    /// Returns the wrapped lister error unchanged.
    async fn next(&mut self) -> opendal::Result<Option<oio::Entry>> {
        let result = self.inner.next().await;
        if (result.is_err() || matches!(result, Ok(None)))
            && let Some(active) = self.active.take()
        {
            active.finish(&result);
        }
        result
    }
}

/// Deleter wrapper that keeps the operation active through batch close.
pub(super) struct StorageDeleter<D> {
    inner: D,
    active: Option<ActiveOperation>,
}

impl<D: oio::Delete> oio::Delete for StorageDeleter<D> {
    /// Forward one queued deletion without exposing its path.
    ///
    /// # Errors
    ///
    /// Returns the wrapped deleter's queueing error unchanged.
    async fn delete(&mut self, path: &str, args: OpDelete) -> opendal::Result<()> {
        let result = self.inner.delete(path, args).await;
        if result.is_err()
            && let Some(active) = self.active.take()
        {
            active.finish(&result);
        }
        result
    }

    /// Close the deletion batch and record its unchanged terminal result.
    ///
    /// # Errors
    ///
    /// Returns the wrapped deleter's close error unchanged; the layer does not
    /// infer completion for deletes the backend did not confirm.
    async fn close(&mut self) -> opendal::Result<()> {
        let result = self.inner.close().await;
        if let Some(active) = self.active.take() {
            active.finish(&result);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use metrics::{Counter, Gauge, Histogram, HistogramFn, Key, Metadata, Recorder};
    use opendal::raw::oio::{Delete as _, Write as _};
    use opendal::raw::{OpDelete, oio};
    use opendal::{Buffer, EntryMode, Error, ErrorKind, Metadata as ObjectMetadata};

    use super::{
        ActiveOperation, StorageDeleter, StorageOperation, StorageTelemetryLayer, StorageWriter,
    };

    /// Isolated counter/gauge recorder for exact storage lifecycle assertions.
    #[derive(Default)]
    struct TestRecorder {
        counters: Mutex<HashMap<String, Arc<metrics::atomics::AtomicU64>>>,
        gauges: Mutex<HashMap<String, Arc<metrics::atomics::AtomicU64>>>,
    }

    /// Histogram sink retaining only the number of terminal observations.
    #[derive(Default)]
    struct HistogramCount(AtomicU64);

    impl HistogramFn for HistogramCount {
        fn record(&self, _: f64) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl Recorder for TestRecorder {
        fn describe_counter(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        fn describe_gauge(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        fn describe_histogram(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }

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

        fn register_histogram(&self, _: &Key, _: &Metadata<'_>) -> Histogram {
            Histogram::from_arc(Arc::new(HistogramCount::default()))
        }
    }

    impl TestRecorder {
        /// Read one exact counter series.
        fn counter(&self, key: &str) -> u64 {
            self.counters
                .lock()
                .expect("counter registry")
                .get(key)
                .map_or(0, |value| value.load(Ordering::Relaxed))
        }

        /// Read one exact gauge series.
        fn gauge(&self, key: &str) -> f64 {
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

    /// Deterministic writer with separately controlled write, close, and abort results.
    struct FaultWriter {
        write_fails: bool,
        close_fails: bool,
        abort_fails: bool,
    }

    impl oio::Write for FaultWriter {
        async fn write(&mut self, _: Buffer) -> opendal::Result<()> {
            if self.write_fails {
                Err(test_error("write"))
            } else {
                Ok(())
            }
        }

        async fn close(&mut self) -> opendal::Result<ObjectMetadata> {
            if self.close_fails {
                Err(test_error("close"))
            } else {
                Ok(ObjectMetadata::new(EntryMode::FILE))
            }
        }

        async fn abort(&mut self) -> opendal::Result<()> {
            if self.abort_fails {
                Err(test_error("abort"))
            } else {
                Ok(())
            }
        }
    }

    /// Deterministic queued deleter with independent queue and flush failures.
    struct FaultDeleter {
        delete_fails: bool,
        close_fails: bool,
    }

    impl oio::Delete for FaultDeleter {
        async fn delete(&mut self, _: &str, _: OpDelete) -> opendal::Result<()> {
            if self.delete_fails {
                Err(test_error("delete"))
            } else {
                Ok(())
            }
        }

        async fn close(&mut self) -> opendal::Result<()> {
            if self.close_fails {
                Err(test_error("flush"))
            } else {
                Ok(())
            }
        }
    }

    /// Construct one stable test error whose identity must survive forwarding.
    fn test_error(operation: &'static str) -> Error {
        Error::new(ErrorKind::Unexpected, operation)
    }

    /// Create one instrumented test writer.
    fn writer(inner: FaultWriter) -> StorageWriter<FaultWriter> {
        StorageWriter {
            inner,
            active: Some(ActiveOperation::start("local", StorageOperation::Put)),
            pending_bytes: 0,
        }
    }

    /// Successful close publishes accumulated bytes once and drains active work.
    #[tokio::test(flavor = "current_thread")]
    async fn writer_publishes_multichunk_bytes_only_after_successful_close() {
        let recorder = TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let mut writer = writer(FaultWriter {
            write_fails: false,
            close_fails: false,
            abort_fails: false,
        });
        writer
            .write(Buffer::from(vec![1_u8; 3]))
            .await
            .expect("first chunk");
        writer
            .write(Buffer::from(vec![2_u8; 5]))
            .await
            .expect("second chunk");
        assert_eq!(
            recorder
                .counter("wyrd_storage_bytes_total{backend=local,direction=write,operation=put}"),
            0
        );
        writer.close().await.expect("commit writer");
        assert_eq!(
            recorder
                .counter("wyrd_storage_bytes_total{backend=local,direction=write,operation=put}"),
            8
        );
        assert_eq!(
            recorder.counter(
                "wyrd_storage_operations_total{backend=local,operation=put,outcome=success}"
            ),
            1
        );
        assert!(
            recorder
                .gauge("wyrd_storage_operations_active{backend=local,operation=put}")
                .abs()
                <= f64::EPSILON
        );
    }

    /// Close failure preserves the error and publishes no accepted bytes.
    #[tokio::test(flavor = "current_thread")]
    async fn writer_close_failure_is_failed_with_zero_published_bytes() {
        let recorder = TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let mut writer = writer(FaultWriter {
            write_fails: false,
            close_fails: true,
            abort_fails: false,
        });
        writer
            .write(Buffer::from(vec![1_u8; 7]))
            .await
            .expect("accepted chunk");
        assert!(
            writer
                .close()
                .await
                .expect_err("close failure")
                .to_string()
                .contains("close")
        );
        assert_eq!(
            recorder
                .counter("wyrd_storage_bytes_total{backend=local,direction=write,operation=put}"),
            0
        );
        assert_eq!(
            recorder.counter(
                "wyrd_storage_operations_total{backend=local,operation=put,outcome=failed}"
            ),
            1
        );
    }

    /// Successful abort is cancellation, while abort failure is failed exactly once.
    #[tokio::test(flavor = "current_thread")]
    async fn writer_abort_outcomes_are_typed_and_publish_no_bytes() {
        let recorder = TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let mut cancelled = writer(FaultWriter {
            write_fails: false,
            close_fails: false,
            abort_fails: false,
        });
        cancelled
            .write(Buffer::from(vec![1_u8; 4]))
            .await
            .expect("accepted chunk");
        cancelled.abort().await.expect("abort succeeds");
        let mut failed = writer(FaultWriter {
            write_fails: false,
            close_fails: false,
            abort_fails: true,
        });
        assert!(
            failed
                .abort()
                .await
                .expect_err("abort failure")
                .to_string()
                .contains("abort")
        );
        assert_eq!(
            recorder
                .counter("wyrd_storage_bytes_total{backend=local,direction=write,operation=put}"),
            0
        );
        assert_eq!(
            recorder.counter(
                "wyrd_storage_operations_total{backend=local,operation=put,outcome=cancelled}"
            ),
            1
        );
        assert_eq!(
            recorder.counter(
                "wyrd_storage_operations_total{backend=local,operation=put,outcome=failed}"
            ),
            1
        );
        assert!(
            recorder
                .gauge("wyrd_storage_operations_active{backend=local,operation=put}")
                .abs()
                <= f64::EPSILON
        );
    }

    /// Write failure and dropped writer terminate once without publishing bytes.
    #[tokio::test(flavor = "current_thread")]
    async fn writer_failure_and_drop_are_failed_and_cancelled_without_bytes() {
        let recorder = TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let mut failed = writer(FaultWriter {
            write_fails: true,
            close_fails: false,
            abort_fails: false,
        });
        assert!(
            failed
                .write(Buffer::from(vec![1_u8; 9]))
                .await
                .expect_err("write failure")
                .to_string()
                .contains("write")
        );
        drop(writer(FaultWriter {
            write_fails: false,
            close_fails: false,
            abort_fails: false,
        }));
        assert_eq!(
            recorder
                .counter("wyrd_storage_bytes_total{backend=local,direction=write,operation=put}"),
            0
        );
        assert_eq!(
            recorder.counter(
                "wyrd_storage_operations_total{backend=local,operation=put,outcome=failed}"
            ),
            1
        );
        assert_eq!(
            recorder.counter(
                "wyrd_storage_operations_total{backend=local,operation=put,outcome=cancelled}"
            ),
            1
        );
        assert!(
            recorder
                .gauge("wyrd_storage_operations_active{backend=local,operation=put}")
                .abs()
                <= f64::EPSILON
        );
    }

    /// Direct and cloned operators traverse the one installed telemetry layer once.
    #[tokio::test(flavor = "current_thread")]
    async fn direct_and_cloned_memory_operators_emit_exact_success_and_bytes() {
        let recorder = TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let operator = opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish()
            .layer(StorageTelemetryLayer::new(
                wyrd_spec::storage::StorageBackendKind::Local,
            ));
        operator
            .write("direct", vec![1_u8; 3])
            .await
            .expect("direct write");
        let clone = operator.clone();
        clone
            .write("clone", vec![2_u8; 5])
            .await
            .expect("clone write");
        assert_eq!(operator.read("direct").await.expect("direct read").len(), 3);
        clone.stat("clone").await.expect("clone stat");
        let _entries = operator.list("").await.expect("root list");
        clone.delete("clone").await.expect("clone delete");
        assert_eq!(
            recorder
                .counter("wyrd_storage_bytes_total{backend=local,direction=write,operation=put}"),
            8
        );
        assert_eq!(
            recorder.counter(
                "wyrd_storage_operations_total{backend=local,operation=put,outcome=success}"
            ),
            2
        );
        for operation in ["get", "list", "delete", "head"] {
            assert_eq!(
                recorder.counter(&format!(
                    "wyrd_storage_operations_total{{backend=local,operation={operation},outcome=success}}"
                )),
                1
            );
            assert!(
                recorder
                    .gauge(&format!(
                        "wyrd_storage_operations_active{{backend=local,operation={operation}}}"
                    ))
                    .abs()
                    <= f64::EPSILON
            );
        }
    }

    /// Queue and flush failures retain their error and cannot fall through to cancellation.
    #[tokio::test(flavor = "current_thread")]
    async fn deleter_failures_are_failed_exactly_once() {
        let recorder = TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let mut queued = StorageDeleter {
            inner: FaultDeleter {
                delete_fails: true,
                close_fails: false,
            },
            active: Some(ActiveOperation::start("local", StorageOperation::Delete)),
        };
        assert!(
            queued
                .delete("secret-path", OpDelete::default())
                .await
                .expect_err("queue failure")
                .to_string()
                .contains("delete")
        );
        let mut flushed = StorageDeleter {
            inner: FaultDeleter {
                delete_fails: false,
                close_fails: true,
            },
            active: Some(ActiveOperation::start("local", StorageOperation::Delete)),
        };
        flushed
            .delete("secret-path", OpDelete::default())
            .await
            .expect("queue succeeds");
        assert!(
            flushed
                .close()
                .await
                .expect_err("flush failure")
                .to_string()
                .contains("flush")
        );
        assert_eq!(
            recorder.counter(
                "wyrd_storage_operations_total{backend=local,operation=delete,outcome=failed}"
            ),
            2
        );
        assert_eq!(
            recorder.counter(
                "wyrd_storage_operations_total{backend=local,operation=delete,outcome=cancelled}"
            ),
            0
        );
        assert!(
            recorder
                .gauge("wyrd_storage_operations_active{backend=local,operation=delete}")
                .abs()
                <= f64::EPSILON
        );
        assert!(
            !recorder
                .counters
                .lock()
                .expect("counter registry")
                .keys()
                .any(|key| key.contains("secret-path"))
        );
    }
}
