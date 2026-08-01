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
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyModule;
use secrecy::SecretString;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{HttpConfig, HttpTransport, ResolvedCredential};
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};

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
    fn next_batch(&mut self) -> Result<Option<RecordBatch>, ValaSdkError> {
        match self {
            Self::Production(stream) => wyrd_runtime::runtime().block_on(stream.next_batch()),
            #[cfg(test)]
            Self::Test(stream) => stream.next_batch(),
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
        }
    }

    /// Returns the next injected result.
    ///
    /// # Errors
    ///
    /// Returns the injected error, or [`ValaSdkError::IncompleteQueryStream`]
    /// after all injected results have been consumed.
    fn next_batch(&mut self) -> Result<Option<RecordBatch>, ValaSdkError> {
        self.polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
            stream: Mutex::new(Some(NativeStreamOwner::Production(Box::new(stream)))),
            terminal_json: Mutex::new(None),
        })
    }
}

/// Native terminal-validating stream used by Python's async iterator facade.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "_NativeBifrostQueryStream")]
pub struct PyBifrostQueryStream {
    /// Rust stream removed on explicit close or after terminal completion.
    stream: Mutex<Option<NativeStreamOwner>>,
    /// Serialized terminal retained after the Rust stream is released.
    terminal_json: Mutex<Option<String>>,
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
            let mut stream_slot = self
                .stream
                .lock()
                .map_err(|_| PyRuntimeError::new_err("query stream lock poisoned"))?;
            let stream = stream_slot
                .as_mut()
                .ok_or_else(|| PyRuntimeError::new_err("query stream is closed"))?;
            let next = match stream.next_batch() {
                Ok(next) => next,
                Err(error) => {
                    let terminal = stream
                        .terminal()
                        .map(serde_json::to_string)
                        .transpose()
                        .map_err(|error| PyRuntimeError::new_err(error.to_string()));
                    *stream_slot = None;
                    let terminal = terminal?;
                    if let Some(terminal) = terminal {
                        *self
                            .terminal_json
                            .lock()
                            .map_err(|_| PyRuntimeError::new_err("terminal lock poisoned"))? =
                            Some(terminal);
                    }
                    return Err(query_error_to_py(error));
                }
            };
            match next {
                Some(batch) => match encode_batch(&batch) {
                    Ok(batch) => Ok(Some(batch)),
                    Err(error) => {
                        *stream_slot = None;
                        Err(error)
                    }
                },
                None => {
                    let terminal = match stream.terminal() {
                        Some(terminal) => serde_json::to_string(terminal)
                            .map_err(|error| PyRuntimeError::new_err(error.to_string())),
                        None => Err(PyRuntimeError::new_err(
                            "[WYRD_VALA_502_QUERY_STREAM_INCOMPLETE] missing terminal",
                        )),
                    };
                    *stream_slot = None;
                    let terminal = terminal?;
                    *self
                        .terminal_json
                        .lock()
                        .map_err(|_| PyRuntimeError::new_err("terminal lock poisoned"))? =
                        Some(terminal);
                    Ok(None)
                }
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
        *self
            .stream
            .lock()
            .map_err(|_| PyRuntimeError::new_err("query stream lock poisoned"))? = None;
        Ok(())
    }
}

#[cfg(test)]
impl PyBifrostQueryStream {
    /// Wraps an injectable owner for deterministic native cleanup tests.
    fn new_for_test(owner: TestStreamOwner) -> Self {
        Self {
            stream: Mutex::new(Some(NativeStreamOwner::Test(owner))),
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
fn encode_batch(batch: &arrow::record_batch::RecordBatch) -> PyResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, batch.schema().as_ref())
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    writer
        .write(batch)
        .and_then(|()| writer.finish())
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    Ok(bytes)
}

/// Converts one SDK failure into a Python runtime error while preserving its code.
fn query_error_to_py(error: ValaSdkError) -> PyErr {
    let message = format!("[{}] {error}", error.code());
    if matches!(error, ValaSdkError::IncompleteQueryStream) {
        IncompleteQueryStreamError::new_err(message)
    } else {
        BifrostQueryError::new_err(message)
    }
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
        assert!(
            error
                .to_string()
                .contains("WYRD_VALA_500_QUERY_EXECUTION_FAILED")
        );
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
            assert!(error.to_string().contains("WYRD_VALA_"));
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
        assert!(
            error
                .to_string()
                .contains("WYRD_SERVER_503_SERVICE_UNAVAILABLE")
        );
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(poll(&owner).is_err(), "second poll closes");
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
