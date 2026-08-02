//! PyO3 boundary for Vala's Bifrost write and terminal-safe query clients.
//!
//! Every method converts its Python inputs to the native C4b types at the edge
//! (JSON-Schema text to an Arrow schema, a card-ref string to [`CardRef`]) and
//! then calls the Rust-native handle — no queue or engine logic is
//! re-implemented here. [`crate::scope::ProducerKey`]-equivalent pool keys and
//! [`ClientScope`] stay opaque: Python never names them.

use std::sync::{Arc, Mutex};

use arrow::ipc::writer::StreamWriter;
use futures_util::future::{AbortHandle, Abortable};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyModule, PyType};
use secrecy::SecretString;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{HttpConfig, HttpTransport, ResolvedCredential};
use wyrd_queue::QueueConfig;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_utils::py::json_to_pyobject;

use crate::native_owner::NativeStreamOwner;
use crate::{
    BifrostGrpcTransport, BifrostIngestSink, ClientScope, QueryClient, SinkKind, ValaSdkError,
    schema_from_json_schema,
};

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

/// Python-facing Bifrost write handle over the pooled native producers.
///
/// Construction resolves the explicit HTTP server URL and API key together
/// with the configured gRPC endpoint, connects the real ingest transport, and
/// keeps all buffering and producer ownership in the Rust SDK.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "Bifrost")]
pub struct Bifrost {
    /// Rust-native pooled write handle shared with the observe projection.
    handle: crate::Bifrost,
}

#[pymethods]
impl Bifrost {
    /// Connects the native write handle to the configured gRPC ingest endpoint.
    ///
    /// `server_url` selects the HTTP authentication plane. The gRPC endpoint
    /// follows [`ClientConfig::from_env`], including `WYRD_GRPC_URL`, so local
    /// test servers and split-plane deployments can expose distinct ports.
    ///
    /// # Errors
    ///
    /// Raises `ValueError` for empty or invalid configuration and
    /// `RuntimeError` when the authenticated gRPC connection cannot be made.
    #[new]
    #[pyo3(signature = (server_url, api_key))]
    fn __new__(py: Python<'_>, server_url: &str, api_key: &str) -> PyResult<Self> {
        if server_url.trim().is_empty() {
            return Err(PyValueError::new_err("server_url must not be empty"));
        }
        if api_key.is_empty() {
            return Err(PyValueError::new_err("api_key must not be empty"));
        }

        let mut config = ClientConfig::from_env();
        config.http.base_url = server_url.trim_end_matches('/').to_owned();
        config.api_key = Some(SecretString::from(api_key.to_owned()));
        let scope = ClientScope::from_config(&config)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let client = WyrdClient::with_config(config)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let transport = py
            .detach(|| wyrd_runtime::runtime().block_on(BifrostGrpcTransport::connect(&client)))
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let sink = Arc::new(BifrostIngestSink::new(Arc::new(transport)));
        Ok(Self {
            handle: crate::Bifrost::new(scope, sink, QueueConfig::default()),
        })
    }

    /// Explicit write path: parse the JSON schema and card reference, then enqueue one JSON `row`.
    ///
    /// The native producer pool owns batching and the authenticated gRPC sink
    /// owns delivery; this boundary only converts Python strings into native
    /// contract values and preserves queue backpressure.
    ///
    /// # Errors
    /// Raises `ValueError` for malformed JSON schema, unsupported schema nodes,
    /// or an invalid card reference. Raises `RuntimeError` when the bounded
    /// producer queue is full or the producer has begun draining.
    #[pyo3(signature = (table, schema, row, card_ref, run_id=None))]
    fn insert(
        &self,
        table: &str,
        schema: &str,
        row: &str,
        card_ref: &str,
        run_id: Option<String>,
    ) -> PyResult<()> {
        let schema_value: serde_json::Value = serde_json::from_str(schema)
            .map_err(|error| PyValueError::new_err(format!("invalid JSON schema: {error}")))?;
        let schema = schema_from_json_schema(&schema_value)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let card_ref = card_ref
            .parse()
            .map_err(|error| PyValueError::new_err(format!("invalid card_ref: {error}")))?;
        let run_id = run_id.map(wyrd_spec::vala::ids::RunId::from_string);
        self.handle
            .insert(
                SinkKind::Record,
                table,
                &schema,
                row.as_bytes().to_vec(),
                card_ref,
                run_id,
            )
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))
    }

    /// Flush all queued rows and await each native ingest acknowledgement.
    ///
    /// Releases the Python GIL while the native drain performs queue and
    /// network work.
    ///
    /// # Errors
    /// Raises `RuntimeError` when a batch cannot be sealed or acknowledged by
    /// the server, preserving the native queue error text at the boundary.
    fn flush(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.handle.flush())
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))
    }

    /// Drain queued rows and stop every native producer background task.
    ///
    /// Releases the Python GIL while the native terminal drain completes.
    ///
    /// # Errors
    /// Raises `RuntimeError` when the terminal drain cannot complete or a
    /// server acknowledgement reports a write failure.
    fn shutdown(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.handle.shutdown())
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))
    }

    /// Rows dropped by the fire-and-forget observe path under backpressure.
    #[getter]
    fn dropped(&self) -> u64 {
        self.handle.dropped()
    }

    /// Number of distinct producers currently pooled.
    #[getter]
    fn producer_count(&self) -> usize {
        self.handle.producer_count()
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

/// Record one telemetry observation, fire-and-forget.
///
/// Uses the same native handle and producer pool as explicit Bifrost inserts.
///
/// # Errors
/// Raises `ValueError` for malformed JSON schema, unsupported schema nodes, or
/// an invalid card reference. Queue saturation is intentionally swallowed by
/// the native observe path and reflected through the handle's drop counter.
#[pyfunction]
#[pyo3(signature = (bifrost, table, schema, row, card_ref, run_id=None))]
fn record(
    bifrost: PyRef<'_, Bifrost>,
    table: &str,
    schema: &str,
    row: &str,
    card_ref: &str,
    run_id: Option<String>,
) -> PyResult<()> {
    let schema_value: serde_json::Value = serde_json::from_str(schema)
        .map_err(|error| PyValueError::new_err(format!("invalid JSON schema: {error}")))?;
    let schema = schema_from_json_schema(&schema_value)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let card_ref = card_ref
        .parse()
        .map_err(|error| PyValueError::new_err(format!("invalid card_ref: {error}")))?;
    let run_id = run_id.map(wyrd_spec::vala::ids::RunId::from_string);
    crate::observe::record(
        &bifrost.handle,
        SinkKind::Record,
        table,
        &schema,
        row.as_bytes().to_vec(),
        card_ref,
        run_id,
    );
    Ok(())
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
