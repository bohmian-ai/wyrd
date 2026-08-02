//! PyO3 boundary for Vala's Bifrost write and terminal-safe query clients.
//!
//! Every method converts its Python inputs to the native C4b types at the edge
//! (JSON-Schema text to an Arrow schema, a card-ref string to [`CardRef`]) and
//! then calls the Rust-native handle — no queue or engine logic is
//! re-implemented here. [`crate::scope::ProducerKey`]-equivalent pool keys and
//! [`ClientScope`] stay opaque: Python never names them.

use std::sync::{Arc, Mutex};

use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use futures_util::future::{AbortHandle, Abortable};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyModule, PyType};
use secrecy::SecretString;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{HttpConfig, HttpTransport, ResolvedCredential};
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_utils::py::json_to_pyobject;

use crate::{QueryClient, QueryResultStream, ValaSdkError};

/// Rust-owned stream implementation used by the Python native owner.
enum NativeStreamOwner {
    /// Production Vala stream.
    Production(Box<QueryResultStream>),
    #[cfg(test)]
    /// Injectable owner used only by deterministic native-owner tests.
    Test(TestStreamOwner),
}

impl NativeStreamOwner {
    /// Polls one decoded batch or terminal state from the underlying owner.
    ///
    /// # Errors
    ///
    /// Returns the owner's protocol, Arrow, transport, terminal, or
    /// incomplete-stream error without changing its retained terminal state.
    async fn next_batch(&mut self) -> Result<Option<RecordBatch>, ValaSdkError> {
        match self {
            Self::Production(stream) => stream.next_batch().await,
            #[cfg(test)]
            Self::Test(stream) => stream.next_batch().await,
        }
    }

    /// Returns terminal metadata retained by the underlying owner.
    fn terminal(&self) -> Option<&wyrd_spec::vala::api::QueryTerminalFrame> {
        match self {
            Self::Production(stream) => stream.terminal(),
            #[cfg(test)]
            Self::Test(stream) => stream.terminal(),
        }
    }
}

#[cfg(test)]
/// Deterministic stream owner used to exercise native cleanup branches.
struct TestStreamOwner {
    /// Results returned by successive native polls.
    next: std::collections::VecDeque<Result<Option<RecordBatch>, ValaSdkError>>,
    /// Terminal metadata exposed while projecting a failed or successful end.
    terminal: Option<wyrd_spec::vala::api::QueryTerminalFrame>,
    /// Sentinel set when the native owner drops this stream.
    dropped: Arc<std::sync::atomic::AtomicBool>,
    /// Counts calls into the injected owner so closed polls prove no re-entry.
    polls: Arc<std::sync::atomic::AtomicUsize>,
    /// Optional gate that leaves a poll pending until cancellation drops it.
    pending: Option<TestPendingPoll>,
}

#[cfg(test)]
/// Deterministic pending-poll controls used by cancellation race tests.
struct TestPendingPoll {
    /// Signals that the native poll reached its blocking point.
    entered: Option<std::sync::mpsc::SyncSender<()>>,
    /// Future that remains pending until its sender is dropped by the test.
    release: tokio::sync::oneshot::Receiver<()>,
    /// Sentinel set when cancellation drops the pending future.
    future_dropped: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(test)]
/// Drop sentinel wrapped around a deterministic pending future.
struct PendingDropGuard {
    /// Sentinel set when the pending poll future is cancelled or completes.
    dropped: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(test)]
impl Drop for PendingDropGuard {
    /// Records that the native pending future no longer occupies a worker.
    fn drop(&mut self) {
        self.dropped
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
impl TestStreamOwner {
    /// Creates an owner with injected poll results and terminal metadata.
    fn new(
        next: std::collections::VecDeque<Result<Option<RecordBatch>, ValaSdkError>>,
        terminal: Option<wyrd_spec::vala::api::QueryTerminalFrame>,
        dropped: Arc<std::sync::atomic::AtomicBool>,
        polls: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self {
            next,
            terminal,
            dropped,
            polls,
            pending: None,
        }
    }

    /// Creates an owner whose first poll deterministically blocks until aborted.
    fn pending(
        entered: std::sync::mpsc::SyncSender<()>,
        release: tokio::sync::oneshot::Receiver<()>,
        future_dropped: Arc<std::sync::atomic::AtomicBool>,
        owner_dropped: Arc<std::sync::atomic::AtomicBool>,
        polls: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self {
            next: std::collections::VecDeque::new(),
            terminal: None,
            dropped: owner_dropped,
            polls,
            pending: Some(TestPendingPoll {
                entered: Some(entered),
                release,
                future_dropped,
            }),
        }
    }

    /// Returns the next injected result.
    ///
    /// # Errors
    ///
    /// Returns the injected error, or [`ValaSdkError::IncompleteQueryStream`]
    /// after all injected results have been consumed.
    async fn next_batch(&mut self) -> Result<Option<RecordBatch>, ValaSdkError> {
        self.polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(mut pending) = self.pending.take() {
            let _drop_guard = PendingDropGuard {
                dropped: Arc::clone(&pending.future_dropped),
            };
            if let Some(entered) = pending.entered.take() {
                let _ = entered.send(());
            }
            let _ = pending.release.await;
        }
        self.next
            .pop_front()
            .unwrap_or(Err(ValaSdkError::IncompleteQueryStream))
    }

    /// Returns injected terminal metadata.
    fn terminal(&self) -> Option<&wyrd_spec::vala::api::QueryTerminalFrame> {
        self.terminal.as_ref()
    }
}

#[cfg(test)]
impl Drop for TestStreamOwner {
    /// Marks the sentinel before the native method returns its error.
    fn drop(&mut self) {
        self.dropped
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

pyo3::create_exception!(
    wyrd._wyrd.bifrost,
    BifrostQueryError,
    pyo3::exceptions::PyRuntimeError,
    "Base exception for terminal-safe Bifrost query failures."
);
pyo3::create_exception!(
    wyrd._wyrd.bifrost,
    IncompleteQueryStreamError,
    BifrostQueryError,
    "Raised when EOF arrives before the required query terminal."
);

/// Python-facing Bifrost write handle over the pooled C4b producers.
///
/// The networked ingest transport is not yet wired — constructing this class
/// raises `RuntimeError` until the gRPC ingest path lands. Callers should catch
/// the error and disable any code paths that require live Bifrost writes.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "Bifrost")]
pub struct Bifrost {
    _private: (),
}

#[pymethods]
impl Bifrost {
    /// Not yet available — raises `RuntimeError` until gRPC ingest is wired.
    #[new]
    #[pyo3(signature = (_server_url, _api_key))]
    fn __new__(_server_url: String, _api_key: String) -> PyResult<Self> {
        Err(PyRuntimeError::new_err(
            "bifrost transport unavailable: gRPC ingest not wired",
        ))
    }

    /// Explicit write path: enqueue one JSON `row`, propagating queue-full.
    #[pyo3(signature = (table, schema, row, card_ref, run_id=None))]
    #[allow(unused_variables)]
    fn insert(
        &self,
        table: &str,
        schema: &str,
        row: &str,
        card_ref: &str,
        run_id: Option<String>,
    ) -> PyResult<()> {
        Err(PyRuntimeError::new_err(
            "bifrost transport unavailable: gRPC ingest not wired",
        ))
    }

    /// Rows dropped by the fire-and-forget observe path under backpressure.
    #[getter]
    fn dropped(&self) -> u64 {
        0
    }

    /// Number of distinct producers currently pooled.
    #[getter]
    fn producer_count(&self) -> usize {
        0
    }
}

/// Native query client used by the thin public Python async facade.
///
/// The public package runs these blocking entry points through
/// `asyncio.to_thread`; each entry point releases the GIL and drives Rust IO on
/// Wyrd's process-wide runtime.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "_NativeBifrostQueryClient")]
pub struct PyBifrostQueryClient {
    /// Rust owner sharing one authenticated HTTP connection pool.
    client: QueryClient,
}

#[pymethods]
impl PyBifrostQueryClient {
    /// Constructs a token-authenticated query client without performing IO.
    ///
    /// # Errors
    ///
    /// Raises `ValueError` when the server URL is empty or transport
    /// configuration is invalid.
    #[new]
    #[pyo3(signature = (server_url, token))]
    fn __new__(server_url: &str, token: &str) -> PyResult<Self> {
        if server_url.trim().is_empty() {
            return Err(PyValueError::new_err("server_url must not be empty"));
        }
        if token.is_empty() {
            return Err(PyValueError::new_err("token must not be empty"));
        }
        let config = ClientConfig {
            http: HttpConfig {
                base_url: server_url.trim_end_matches('/').to_owned(),
                ..HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(SecretString::from(token.to_owned())),
        )
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let http = HttpTransport::new(&config.http, Arc::clone(&auth))
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let client = WyrdClient::from_parts(auth, http, config.grpc);
        Ok(Self {
            client: QueryClient::new(&client),
        })
    }

    /// Starts one query through the shared Rust runtime.
    ///
    /// Python's public async facade calls this method in a worker thread, so
    /// event-loop execution is never blocked.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` for transport or request-contract failures.
    #[pyo3(signature = (sql, visibility="published_only", freshness="strict", deadline_ms=None))]
    fn query(
        &self,
        py: Python<'_>,
        sql: &str,
        visibility: &str,
        freshness: &str,
        deadline_ms: Option<u64>,
    ) -> PyResult<PyBifrostQueryStream> {
        let request = BifrostQueryRequest {
            sql: sql.to_owned(),
            visibility: parse_visibility(visibility)?,
            freshness: parse_freshness(freshness)?,
            deadline_ms,
        };
        let stream = py
            .detach(|| wyrd_runtime::runtime().block_on(self.client.query(&request)))
            .map_err(query_error_to_py)?;
        Ok(PyBifrostQueryStream {
            consumer: Mutex::new(()),
            stream: Mutex::new(Some(NativeStreamOwner::Production(Box::new(stream)))),
            poll_abort: Mutex::new(PollAbortState::new()),
            terminal_json: Mutex::new(None),
        })
    }
}

/// Native terminal-validating stream used by Python's async iterator facade.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "_NativeBifrostQueryStream")]
pub struct PyBifrostQueryStream {
    /// Serializes consumers without blocking cancellation signalling.
    consumer: Mutex<()>,
    /// Rust stream removed on explicit close or after terminal completion.
    stream: Mutex<Option<NativeStreamOwner>>,
    /// Short-held state for aborting the currently pending native poll.
    poll_abort: Mutex<PollAbortState>,
    /// Serialized terminal retained after the Rust stream is released.
    terminal_json: Mutex<Option<String>>,
}

/// Abort state for exactly one serialized native query poll.
struct PollAbortState {
    /// Monotonic identity used to avoid clearing a replacement handle.
    next_id: u64,
    /// Identity and handle for the poll currently entering or awaiting IO.
    current: Option<(u64, AbortHandle)>,
}

impl PollAbortState {
    /// Creates empty abort state for a newly opened stream.
    fn new() -> Self {
        Self {
            next_id: 0,
            current: None,
        }
    }

    /// Installs one handle and returns its identity.
    fn install(&mut self, handle: AbortHandle) -> u64 {
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        self.current = Some((id, handle));
        id
    }

    /// Clears the handle only when it still belongs to the completed poll.
    fn clear(&mut self, id: u64) {
        if matches!(self.current, Some((current, _)) if current == id) {
            self.current = None;
        }
    }

    /// Takes and signals the pending poll without retaining the state lock.
    fn abort_current(&mut self) {
        if let Some((_, handle)) = self.current.take() {
            handle.abort();
        }
    }
}

/// Rust-native poll outcome retained until the response owner has dropped.
enum NativePollOutcome {
    /// One Arrow batch encoded successfully.
    Batch(Vec<u8>),
    /// The stream completed with serialized terminal metadata.
    Complete(String),
    /// The stream was already closed before polling began.
    Closed,
    /// Explicit close aborted the pending network poll.
    Aborted,
    /// The SDK returned a structured query failure.
    QueryError {
        /// Structured SDK error projected after native ownership is released.
        error: ValaSdkError,
        /// Optional serialized terminal retained before projecting the error.
        terminal: Option<String>,
    },
    /// Local serialization or Arrow encoding failed.
    ProjectionError(String),
}

#[pymethods]
impl PyBifrostQueryStream {
    /// Returns the next Arrow IPC batch, or `None` after validated completion.
    ///
    /// The public Python wrapper converts the returned bytes into one public
    /// `pyarrow.RecordBatch`.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd-coded runtime error for protocol, terminal, transport,
    /// Arrow, or poisoned-state failures.
    fn next_ipc(&self, py: Python<'_>) -> PyResult<Option<Vec<u8>>> {
        py.detach(|| {
            let _consumer = self
                .consumer
                .lock()
                .map_err(|_| PyRuntimeError::new_err("query consumer lock poisoned"))?;
            let (abort_handle, abort_registration) = AbortHandle::new_pair();
            let poll_id = self
                .poll_abort
                .lock()
                .map_err(|_| PyRuntimeError::new_err("query abort lock poisoned"))?
                .install(abort_handle);

            let outcome = match self.stream.lock() {
                Ok(mut stream_slot) => match stream_slot.as_mut() {
                    None => NativePollOutcome::Closed,
                    Some(stream) => {
                        let next = wyrd_runtime::runtime()
                            .block_on(Abortable::new(stream.next_batch(), abort_registration));
                        match next {
                            Err(_) => {
                                drop(stream_slot.take());
                                NativePollOutcome::Aborted
                            }
                            Ok(Err(error)) => {
                                let terminal = stream
                                    .terminal()
                                    .map(serde_json::to_string)
                                    .transpose()
                                    .map_err(|error| error.to_string());
                                drop(stream_slot.take());
                                match terminal {
                                    Ok(terminal) => {
                                        NativePollOutcome::QueryError { error, terminal }
                                    }
                                    Err(error) => NativePollOutcome::ProjectionError(error),
                                }
                            }
                            Ok(Ok(Some(batch))) => match encode_batch(&batch) {
                                Ok(batch) => NativePollOutcome::Batch(batch),
                                Err(error) => {
                                    drop(stream_slot.take());
                                    NativePollOutcome::ProjectionError(error)
                                }
                            },
                            Ok(Ok(None)) => {
                                let terminal = stream
                                    .terminal()
                                    .map(serde_json::to_string)
                                    .transpose()
                                    .map_err(|error| error.to_string())
                                    .and_then(|terminal| {
                                        terminal.ok_or_else(|| {
                                            ValaSdkError::IncompleteQueryStream.detail()
                                        })
                                    });
                                drop(stream_slot.take());
                                match terminal {
                                    Ok(terminal) => NativePollOutcome::Complete(terminal),
                                    Err(error)
                                        if error
                                            == ValaSdkError::IncompleteQueryStream.detail() =>
                                    {
                                        NativePollOutcome::QueryError {
                                            error: ValaSdkError::IncompleteQueryStream,
                                            terminal: None,
                                        }
                                    }
                                    Err(error) => NativePollOutcome::ProjectionError(error),
                                }
                            }
                        }
                    }
                },
                Err(_) => NativePollOutcome::ProjectionError("query stream lock poisoned".into()),
            };

            self.poll_abort
                .lock()
                .map_err(|_| PyRuntimeError::new_err("query abort lock poisoned"))?
                .clear(poll_id);

            match outcome {
                NativePollOutcome::Batch(batch) => Ok(Some(batch)),
                NativePollOutcome::Complete(terminal) => {
                    *self
                        .terminal_json
                        .lock()
                        .map_err(|_| PyRuntimeError::new_err("terminal lock poisoned"))? =
                        Some(terminal);
                    Ok(None)
                }
                NativePollOutcome::Closed => Err(PyRuntimeError::new_err("query stream is closed")),
                NativePollOutcome::Aborted => {
                    Err(PyRuntimeError::new_err("query stream poll aborted"))
                }
                NativePollOutcome::QueryError { error, terminal } => {
                    if let Some(terminal) = terminal {
                        *self
                            .terminal_json
                            .lock()
                            .map_err(|_| PyRuntimeError::new_err("terminal lock poisoned"))? =
                            Some(terminal);
                    }
                    Err(query_error_to_py(error))
                }
                NativePollOutcome::ProjectionError(error) => Err(PyRuntimeError::new_err(error)),
            }
        })
    }

    /// Returns serialized terminal metadata after validated completion.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` when internal synchronization was poisoned.
    #[getter]
    fn terminal_json(&self) -> PyResult<Option<String>> {
        self.terminal_json
            .lock()
            .map(|terminal| terminal.clone())
            .map_err(|_| PyRuntimeError::new_err("terminal lock poisoned"))
    }

    /// Explicitly drops the Rust response stream to propagate cancellation.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` when internal synchronization was poisoned.
    fn close(&self) -> PyResult<()> {
        self.poll_abort
            .lock()
            .map_err(|_| PyRuntimeError::new_err("query abort lock poisoned"))?
            .abort_current();
        drop(
            self.stream
                .lock()
                .map_err(|_| PyRuntimeError::new_err("query stream lock poisoned"))?
                .take(),
        );
        Ok(())
    }

    /// Raises the typed incomplete-stream exception for facade invariant failures.
    ///
    /// The pure Python facade uses this only if a native poll returns completion
    /// without the terminal JSON that the native owner promises to retain.
    ///
    /// # Errors
    ///
    /// Always raises [`IncompleteQueryStreamError`] with metadata projected by
    /// the Rust-native [`ValaSdkError`] seam.
    fn raise_incomplete_error(&self) -> PyResult<()> {
        Err(query_error_to_py(ValaSdkError::IncompleteQueryStream))
    }
}

#[cfg(test)]
impl PyBifrostQueryStream {
    /// Wraps an injectable owner for deterministic native cleanup tests.
    fn new_for_test(owner: TestStreamOwner) -> Self {
        Self {
            consumer: Mutex::new(()),
            stream: Mutex::new(Some(NativeStreamOwner::Test(owner))),
            poll_abort: Mutex::new(PollAbortState::new()),
            terminal_json: Mutex::new(None),
        }
    }
}

/// Record one telemetry observation, fire-and-forget.
///
/// Not yet available — raises `RuntimeError` until gRPC ingest is wired.
#[pyfunction]
#[pyo3(signature = (bifrost, table, schema, row, card_ref, run_id=None))]
#[allow(unused_variables)]
fn record(
    bifrost: PyRef<'_, Bifrost>,
    table: &str,
    schema: &str,
    row: &str,
    card_ref: &str,
    run_id: Option<String>,
) -> PyResult<()> {
    Err(PyRuntimeError::new_err(
        "bifrost transport unavailable: gRPC ingest not wired",
    ))
}

/// Register the `bifrost` write handle on the supplied module.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn register_bifrost(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Bifrost>()?;
    module.add_class::<PyBifrostQueryClient>()?;
    module.add_class::<PyBifrostQueryStream>()?;
    module.add(
        "BifrostQueryError",
        module.py().get_type::<BifrostQueryError>(),
    )?;
    module.add(
        "IncompleteQueryStreamError",
        module.py().get_type::<IncompleteQueryStreamError>(),
    )?;
    Ok(())
}

/// Register the `observe` fire-and-forget telemetry function on the module.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn register_observe(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(record, module)?)?;
    Ok(())
}

/// Parses the Python visibility spelling into the closed Rust contract.
///
/// # Errors
///
/// Raises `ValueError` for any value other than `published_only`,
/// `published-only`, or `fused`.
fn parse_visibility(value: &str) -> PyResult<VisibilityMode> {
    match value {
        "published_only" | "published-only" => Ok(VisibilityMode::PublishedOnly),
        "fused" => Ok(VisibilityMode::Fused),
        _ => Err(PyValueError::new_err(
            "visibility must be 'published_only' or 'fused'",
        )),
    }
}

/// Parses the Python freshness spelling into the closed Rust contract.
///
/// # Errors
///
/// Raises `ValueError` for any value other than `strict`,
/// `allow_degraded`, or `allow-degraded`.
fn parse_freshness(value: &str) -> PyResult<FreshnessPolicy> {
    match value {
        "strict" => Ok(FreshnessPolicy::Strict),
        "allow_degraded" | "allow-degraded" => Ok(FreshnessPolicy::AllowDegraded),
        _ => Err(PyValueError::new_err(
            "freshness must be 'strict' or 'allow_degraded'",
        )),
    }
}

/// Encodes one Rust record batch for the public Python PyArrow conversion.
///
/// # Errors
///
/// Raises a Wyrd-coded `RuntimeError` when Arrow IPC encoding fails.
fn encode_batch(batch: &arrow::record_batch::RecordBatch) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, batch.schema().as_ref())
        .map_err(|error| error.to_string())?;
    writer
        .write(batch)
        .and_then(|()| writer.finish())
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

/// Converts one SDK failure into its typed Python exception with stable metadata.
///
/// # Panics
///
/// Panics only if PyO3 stops allowing attributes on registered exception
/// instances or the existing JSON-to-Python converter rejects valid JSON.
fn query_error_to_py(error: ValaSdkError) -> PyErr {
    Python::attach(|py| {
        let exception_type: Bound<'_, PyType> =
            if matches!(error, ValaSdkError::IncompleteQueryStream) {
                py.get_type::<IncompleteQueryStreamError>()
            } else {
                py.get_type::<BifrostQueryError>()
            };
        let detail = error.detail();
        let instance = exception_type
            .call1((detail.clone(),))
            .expect("invariant: registered query exception constructs from one string");
        instance
            .setattr("code", error.code())
            .expect("invariant: Python exception accepts stable code");
        instance
            .setattr("status", error.status())
            .expect("invariant: Python exception accepts status");
        instance
            .setattr("title", error.title())
            .expect("invariant: Python exception accepts title");
        instance
            .setattr("message", detail.clone())
            .expect("invariant: Python exception accepts message");
        instance
            .setattr("detail", detail)
            .expect("invariant: Python exception accepts detail");
        instance
            .setattr("remediation", error.remediation())
            .expect("invariant: Python exception accepts remediation");
        let details = error.safe_details().unwrap_or(serde_json::Value::Null);
        let details = json_to_pyobject(py, &details)
            .expect("invariant: scrubbed JSON details convert to Python");
        instance
            .setattr("details", details.bind(py))
            .expect("invariant: Python exception accepts details");
        PyErr::from_value(instance)
    })
}

#[cfg(all(test, feature = "python"))]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use pyo3::Python;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::vala::api::{
        QueryErrorDetail, QueryFreshness, QuerySource, QueryTerminalError, QueryTerminalErrorCode,
        QueryTerminalFrame, QueryTerminalOutcome, SourceCompletion, SourceCompletionOutcome,
    };

    use super::*;

    /// Builds the complete published-only terminal used by lifecycle tests.
    fn success_terminal() -> QueryTerminalFrame {
        QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Success,
            freshness: QueryFreshness::Complete,
            row_count: 0,
            warnings: Vec::new(),
            source_completion: vec![
                SourceCompletion {
                    source: QuerySource::Iceberg,
                    outcome: SourceCompletionOutcome::Complete,
                },
                SourceCompletion {
                    source: QuerySource::HotSealed,
                    outcome: SourceCompletionOutcome::Complete,
                },
            ],
            error: None,
        }
    }

    /// Python query exceptions retain their subtype and every typed metadata field.
    #[test]
    fn query_error_projection_preserves_structured_metadata() {
        Python::initialize();
        Python::attach(|py| {
            let error = ValaSdkError::Transport(WyrdError::PermissionDeniedRbac {
                message: "query denied".to_owned(),
                details: serde_json::json!({"required_scope": "bifrost_query:read"}),
            });
            let projected = query_error_to_py(error);
            let value = projected.value(py);
            assert!(value.is_instance_of::<BifrostQueryError>());
            assert_eq!(
                value
                    .getattr("code")
                    .expect("code")
                    .extract::<String>()
                    .expect("string"),
                "WYRD_PERMISSION_403_DENIED_RBAC"
            );
            assert_eq!(
                value
                    .getattr("status")
                    .expect("status")
                    .extract::<u16>()
                    .expect("u16"),
                403
            );
            assert_eq!(
                value
                    .getattr("detail")
                    .expect("detail")
                    .extract::<String>()
                    .expect("string"),
                "query denied"
            );
            let details = value.getattr("details").expect("details");
            assert_eq!(
                details
                    .get_item("required_scope")
                    .expect("scope")
                    .extract::<String>()
                    .expect("string"),
                "bifrost_query:read"
            );

            let incomplete = query_error_to_py(ValaSdkError::IncompleteQueryStream);
            let incomplete_value = incomplete.value(py);
            assert!(incomplete_value.is_instance_of::<IncompleteQueryStreamError>());
            assert_eq!(
                incomplete_value
                    .getattr("code")
                    .expect("code")
                    .extract::<String>()
                    .expect("string"),
                "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE"
            );
            assert_eq!(
                incomplete_value
                    .getattr("status")
                    .expect("status")
                    .extract::<u16>()
                    .expect("u16"),
                502
            );
            assert!(
                incomplete_value
                    .getattr("details")
                    .expect("details")
                    .is_none()
            );

            let unavailable =
                query_error_to_py(ValaSdkError::Transport(WyrdError::ServiceUnavailable {
                    message: "query unavailable".to_owned(),
                    details: serde_json::json!({"retryable": true}),
                }));
            let unavailable_value = unavailable.value(py);
            assert_eq!(
                unavailable_value
                    .getattr("status")
                    .expect("status")
                    .extract::<u16>()
                    .expect("u16"),
                503
            );
            assert_eq!(
                unavailable_value
                    .getattr("detail")
                    .expect("detail")
                    .extract::<String>()
                    .expect("string"),
                "query unavailable"
            );

            let terminal = query_error_to_py(ValaSdkError::FailedTerminal {
                terminal: failed_terminal(),
            });
            let terminal_value = terminal.value(py);
            assert_eq!(
                terminal_value
                    .getattr("status")
                    .expect("status")
                    .extract::<u16>()
                    .expect("u16"),
                500
            );
            assert_eq!(
                terminal_value
                    .getattr("detail")
                    .expect("detail")
                    .extract::<String>()
                    .expect("string"),
                "python native sentinel failure"
            );
            assert!(
                terminal_value
                    .getattr("details")
                    .expect("details")
                    .is_instance_of::<pyo3::types::PyDict>()
            );
        });
    }

    /// Calls one native Python poll while holding the interpreter.
    ///
    /// # Errors
    ///
    /// Returns the native stream's projected Python error when the poll cannot
    /// produce a batch or validated terminal.
    fn poll(owner: &PyBifrostQueryStream) -> PyResult<Option<Vec<u8>>> {
        Python::initialize();
        Python::attach(|py| owner.next_ipc(py))
    }

    /// Builds a failed terminal carrying a scrubbed diagnostic.
    ///
    /// # Panics
    ///
    /// Panics only if the fixed test diagnostic stops satisfying
    /// `QueryErrorDetail`'s scrubbed-value invariant.
    fn failed_terminal() -> QueryTerminalFrame {
        QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            freshness: QueryFreshness::Complete,
            row_count: 0,
            warnings: Vec::new(),
            source_completion: vec![
                SourceCompletion {
                    source: QuerySource::Iceberg,
                    outcome: SourceCompletionOutcome::Complete,
                },
                SourceCompletion {
                    source: QuerySource::HotSealed,
                    outcome: SourceCompletionOutcome::Complete,
                },
            ],
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: Some(
                    QueryErrorDetail::new("python native sentinel failure").expect("detail"),
                ),
            }),
        }
    }

    /// Failed terminals retain diagnostics and drop their native owner first.
    #[test]
    fn native_owner_failed_terminal_drops_before_error_and_closes() {
        let dropped = std::sync::Arc::new(AtomicBool::new(false));
        let polls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let terminal = failed_terminal();
        let owner = PyBifrostQueryStream::new_for_test(TestStreamOwner::new(
            std::collections::VecDeque::from([Err(ValaSdkError::FailedTerminal {
                terminal: terminal.clone(),
            })]),
            Some(terminal),
            std::sync::Arc::clone(&dropped),
            std::sync::Arc::clone(&polls),
        ));
        let error = poll(&owner).expect_err("failed terminal is projected");
        Python::attach(|py| {
            assert_eq!(
                error
                    .value(py)
                    .getattr("code")
                    .expect("code")
                    .extract::<String>()
                    .expect("string"),
                "WYRD_VALA_500_QUERY_EXECUTION_FAILED"
            );
        });
        assert!(
            owner
                .terminal_json()
                .expect("terminal lock remains healthy")
                .expect("diagnostics retained")
                .contains("python native sentinel failure")
        );
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(
            poll(&owner)
                .expect_err("second poll closes")
                .to_string()
                .contains("query stream is closed")
        );
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    /// Protocol, Arrow, EOF, and source errors release ownership before projection.
    #[test]
    fn native_owner_projection_errors_drop_before_error_and_close() {
        let cases = vec![
            Err(ValaSdkError::Protocol("malformed".to_owned())),
            Err(ValaSdkError::Arrow("malformed Arrow".to_owned())),
            Err(ValaSdkError::IncompleteQueryStream),
        ];
        for result in cases {
            let dropped = std::sync::Arc::new(AtomicBool::new(false));
            let polls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let owner = PyBifrostQueryStream::new_for_test(TestStreamOwner::new(
                std::collections::VecDeque::from([result]),
                None,
                std::sync::Arc::clone(&dropped),
                std::sync::Arc::clone(&polls),
            ));
            let error = poll(&owner).expect_err("projection error is surfaced");
            Python::attach(|py| {
                assert!(
                    error
                        .value(py)
                        .getattr("code")
                        .expect("code")
                        .extract::<String>()
                        .expect("string")
                        .starts_with("WYRD_VALA_")
                );
            });
            assert!(dropped.load(Ordering::SeqCst));
            assert_eq!(polls.load(Ordering::SeqCst), 1);
            assert!(poll(&owner).is_err(), "second poll closes");
            assert_eq!(polls.load(Ordering::SeqCst), 1);
        }
    }

    /// Transport diagnostics survive owner cleanup and the next poll is closed.
    #[test]
    fn native_owner_transport_error_drops_before_error_and_closes() {
        let dropped = std::sync::Arc::new(AtomicBool::new(false));
        let polls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let owner = PyBifrostQueryStream::new_for_test(TestStreamOwner::new(
            std::collections::VecDeque::from([Err(ValaSdkError::Transport(
                WyrdError::ServiceUnavailable {
                    message: "body transport failed".to_owned(),
                    details: serde_json::json!({}),
                },
            ))]),
            None,
            std::sync::Arc::clone(&dropped),
            std::sync::Arc::clone(&polls),
        ));
        let error = poll(&owner).expect_err("transport error projects");
        Python::attach(|py| {
            let value = error.value(py);
            assert_eq!(
                value
                    .getattr("code")
                    .expect("code")
                    .extract::<String>()
                    .expect("string"),
                "WYRD_SERVER_503_SERVICE_UNAVAILABLE"
            );
            assert_eq!(
                value
                    .getattr("status")
                    .expect("status")
                    .extract::<u16>()
                    .expect("u16"),
                503
            );
        });
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(poll(&owner).is_err(), "second poll closes");
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    /// Native completion without a retained terminal uses the typed incomplete seam.
    #[test]
    fn native_owner_missing_terminal_projects_structured_incomplete_error() {
        let owner = PyBifrostQueryStream::new_for_test(TestStreamOwner::new(
            std::collections::VecDeque::from([Ok(None)]),
            None,
            std::sync::Arc::new(AtomicBool::new(false)),
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        ));
        let error = poll(&owner).expect_err("missing terminal rejects");
        Python::attach(|py| {
            let value = error.value(py);
            assert!(value.is_instance_of::<IncompleteQueryStreamError>());
            assert_eq!(
                value
                    .getattr("code")
                    .expect("code")
                    .extract::<String>()
                    .expect("string"),
                "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE"
            );
            assert_eq!(
                value
                    .getattr("status")
                    .expect("status")
                    .extract::<u16>()
                    .expect("u16"),
                502
            );
            assert!(value.getattr("details").expect("details").is_none());
        });
    }

    /// Closing before a poll starts is idempotent and prevents owner re-entry.
    #[test]
    fn native_close_before_poll_drops_owner_without_polling() {
        let dropped = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let owner = PyBifrostQueryStream::new_for_test(TestStreamOwner::new(
            std::collections::VecDeque::new(),
            None,
            Arc::clone(&dropped),
            Arc::clone(&polls),
        ));

        owner.close().expect("first close succeeds");
        owner.close().expect("second close is idempotent");

        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        assert!(
            poll(&owner)
                .expect_err("closed poll rejects")
                .to_string()
                .contains("closed")
        );
        assert_eq!(polls.load(Ordering::SeqCst), 0);
    }

    /// Closing a pending poll aborts its future and leaves no worker blocked.
    #[test]
    fn native_close_during_poll_aborts_and_drops_owner() {
        let future_dropped = Arc::new(AtomicBool::new(false));
        let owner_dropped = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (_release_tx, release_rx) = tokio::sync::oneshot::channel();
        let owner = Arc::new(PyBifrostQueryStream::new_for_test(
            TestStreamOwner::pending(
                entered_tx,
                release_rx,
                Arc::clone(&future_dropped),
                Arc::clone(&owner_dropped),
                Arc::clone(&polls),
            ),
        ));

        std::thread::scope(|scope| {
            let poll_owner = Arc::clone(&owner);
            let (completed_tx, completed_rx) = std::sync::mpsc::sync_channel(1);
            let worker = scope.spawn(move || {
                let result = poll(&poll_owner);
                completed_tx
                    .send(())
                    .expect("bounded completion receiver remains live");
                result
            });
            entered_rx
                .recv()
                .expect("pending poll reports entry without timing guesses");
            owner
                .close()
                .expect("close signals abort before waiting for stream");
            completed_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("native poll completion arrives within the test bound");
            let error = worker
                .join()
                .expect("native poll worker exits")
                .expect_err("aborted poll returns an internal closed outcome");
            assert!(error.to_string().contains("aborted"));
        });

        assert!(future_dropped.load(Ordering::SeqCst));
        assert!(owner_dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(
            poll(&owner).is_err(),
            "follow-up poll is immediately closed"
        );
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(
            owner
                .poll_abort
                .lock()
                .expect("abort state remains healthy")
                .current
                .is_none()
        );
    }

    /// Closing after terminal completion is idempotent and retains diagnostics.
    #[test]
    fn native_terminal_then_close_is_idempotent() {
        let dropped = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let owner = PyBifrostQueryStream::new_for_test(TestStreamOwner::new(
            std::collections::VecDeque::from([Ok(None)]),
            Some(success_terminal()),
            Arc::clone(&dropped),
            Arc::clone(&polls),
        ));

        assert!(poll(&owner).expect("terminal poll succeeds").is_none());
        let terminal = owner
            .terminal_json()
            .expect("terminal lock remains healthy")
            .expect("terminal diagnostics are retained");
        owner.close().expect("first close after terminal succeeds");
        owner.close().expect("second close after terminal succeeds");

        assert_eq!(
            owner.terminal_json().expect("terminal remains readable"),
            Some(terminal)
        );
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    /// A poisoned terminal lock still drops the stream before returning failure.
    #[test]
    fn native_owner_terminal_lock_failure_drops_before_error_and_closes() {
        let dropped = std::sync::Arc::new(AtomicBool::new(false));
        let polls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let owner = PyBifrostQueryStream::new_for_test(TestStreamOwner::new(
            std::collections::VecDeque::from([Ok(None)]),
            Some(QueryTerminalFrame {
                outcome: QueryTerminalOutcome::Success,
                freshness: QueryFreshness::Complete,
                row_count: 0,
                warnings: Vec::new(),
                source_completion: vec![
                    SourceCompletion {
                        source: QuerySource::Iceberg,
                        outcome: SourceCompletionOutcome::Complete,
                    },
                    SourceCompletion {
                        source: QuerySource::HotSealed,
                        outcome: SourceCompletionOutcome::Complete,
                    },
                ],
                error: None,
            }),
            std::sync::Arc::clone(&dropped),
            std::sync::Arc::clone(&polls),
        ));
        let terminal_lock = &owner.terminal_json;
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _guard = terminal_lock.lock().expect("terminal lock acquires");
                    panic!("poison terminal lock for owner test");
                })
                .join()
                .expect_err("poisoning thread panics");
        });
        let error = poll(&owner).expect_err("terminal lock failure projects");
        assert!(error.to_string().contains("terminal lock poisoned"));
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(poll(&owner).is_err(), "second poll closes");
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }
}
