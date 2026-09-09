//! PyO3 boundary for the one Bifrost client.
//!
//! Every method converts its Python inputs to native contract types at the edge
//! — a JSON-Schema document or Arrow IPC schema to [`crate::TableConfig`], a
//! card-ref string to [`wyrd_spec::reference::CardRef`] — and then calls the
//! Rust-native client. No queue, schema-mapping, registration, or query logic
//! is re-implemented here; the pool key and [`crate::ClientScope`] stay opaque,
//! and Python never names them.
//!
//! Every entry point is blocking and releases the GIL. The public package's
//! synchronous `Bifrost` calls them directly and its `AsyncBifrost` runs the
//! same calls through `asyncio.to_thread`, so the two facades share one
//! implementation rather than two.

use std::io::Cursor;
use std::sync::Mutex;

use arrow::ipc::reader::StreamReader;
use arrow::ipc::writer::StreamWriter;
use futures_util::future::{AbortHandle, Abortable};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyModule, PyType};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, GetTraceRequest, PhysicalLayoutWire, QueryGenAiRequest,
    VisibilityMode,
};
use wyrd_spec::vala::ids::RunId;
use wyrd_utils::py::json_to_pyobject;

use crate::native_owner::NativeStreamOwner;
use crate::table::Correlation;
use crate::{QueryClient, ValaSdkError, client_from_options};

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
pyo3::create_exception!(
    wyrd._wyrd.bifrost,
    NoCredentialsError,
    BifrostQueryError,
    "Raised when the credential chain yields nothing for an omitted credential."
);

/// The stable code the credential chain reports when it resolves nothing.
///
/// Named here so the boundary can raise the dedicated Python exception for it
/// without re-deriving the chain or matching on error text.
const NO_CREDENTIALS_CODE: &str = "WYRD_CLIENT_401_NO_CREDENTIALS";

/// Python-facing [`crate::TableConfig`]: one Bifrost table's identity, declared
/// user columns, and requested physical layout.
///
/// The schema arrives either as a JSON Schema document — what
/// `BaseModel.model_json_schema()` produces — or as Arrow IPC bytes from a
/// `pyarrow.Schema`, and is mapped by the same `wyrd-queue` owner every
/// language uses. No fingerprint is computed here: the server mints it, so
/// [`PyTableConfig::resolved`] stays `None` until register or describe.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "TableConfig", from_py_object)]
#[derive(Clone)]
pub struct PyTableConfig {
    /// The native config every write and register call reads.
    inner: crate::TableConfig,
}

#[pymethods]
impl PyTableConfig {
    /// Builds one config from a JSON Schema document.
    ///
    /// `layout_json`, when present, is one serialized `PhysicalLayoutWire`; the
    /// public Python wrapper assembles it from its keyword arguments so the
    /// wire contract stays the only layout shape.
    ///
    /// # Errors
    ///
    /// Raises `ValueError` when the table is not `namespace.name`, the document
    /// is not one mappable JSON Schema, a declared column is server-owned, or
    /// the layout is not one physical-layout declaration.
    #[staticmethod]
    #[pyo3(signature = (table, schema_json, layout_json=None))]
    fn from_json_schema(
        table: &str,
        schema_json: &str,
        layout_json: Option<&str>,
    ) -> PyResult<Self> {
        let schema: serde_json::Value = serde_json::from_str(schema_json)
            .map_err(|error| PyValueError::new_err(format!("invalid JSON schema: {error}")))?;
        let config = crate::TableConfig::from_json_schema(table, &schema)
            .map_err(|error| PyValueError::new_err(error.detail()))?;
        Ok(Self {
            inner: apply_layout(config, layout_json)?,
        })
    }

    /// Builds one config from a `pyarrow.Schema` serialized as Arrow IPC.
    ///
    /// The precision door: `int32`, a non-UTC timestamp, or `decimal128` has no
    /// JSON Schema spelling, so the caller hands over Arrow directly rather
    /// than a document the mapper would have to widen.
    ///
    /// # Errors
    ///
    /// Raises `ValueError` when the bytes are not one Arrow IPC schema, the
    /// table is not `namespace.name`, a declared column is server-owned, or the
    /// layout is not one physical-layout declaration.
    #[staticmethod]
    #[pyo3(signature = (table, schema_ipc, layout_json=None))]
    fn from_arrow_ipc(table: &str, schema_ipc: &[u8], layout_json: Option<&str>) -> PyResult<Self> {
        let schema = decode_schema_ipc(schema_ipc)?;
        let config = crate::TableConfig::from_arrow(table, schema)
            .map_err(|error| PyValueError::new_err(error.detail()))?;
        Ok(Self {
            inner: apply_layout(config, layout_json)?,
        })
    }

    /// Fetches an already-registered table's config by name.
    ///
    /// Every transport argument is optional and resolves through the same chain
    /// the constructor uses when omitted.
    ///
    /// # Errors
    ///
    /// Raises [`NoCredentialsError`] when nothing resolves a credential, and a
    /// typed Bifrost query error for not-found, authorization, transport, or
    /// schema-projection failures.
    #[staticmethod]
    #[pyo3(signature = (table, server_url=None, credential=None, grpc_url=None))]
    fn describe(
        py: Python<'_>,
        table: &str,
        server_url: Option<&str>,
        credential: Option<&str>,
        grpc_url: Option<&str>,
    ) -> PyResult<Self> {
        let client =
            client_from_options(server_url, credential, grpc_url).map_err(query_error_to_py)?;
        let inner = py
            .detach(|| {
                wyrd_runtime::runtime().block_on(crate::TableConfig::describe(&client, table))
            })
            .map_err(query_error_to_py)?;
        Ok(Self { inner })
    }

    /// The `namespace.name` this config addresses.
    #[getter]
    fn fqn(&self) -> String {
        self.inner.fqn()
    }

    /// The declared user columns as one schema-only Arrow IPC stream.
    ///
    /// Python decodes these with its installed Arrow implementation rather than
    /// rebuilding a schema from field declarations, so the two sides cannot
    /// disagree about a column's type.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` when the schema cannot be encoded.
    #[getter]
    fn arrow_schema_ipc(&self) -> PyResult<Vec<u8>> {
        encode_schema_ipc(self.inner.user_schema())
    }

    /// The server-assigned `(table_uid, fingerprint)`, or `None` while inert.
    #[getter]
    fn resolved(&self) -> Option<(String, String)> {
        self.inner
            .resolved()
            .map(|resolved| (resolved.table_uid.clone(), resolved.fingerprint.clone()))
    }
}

/// Python-facing collected query result.
///
/// Holds the decoded Rust batches; [`PyQueryResult::to_ipc`] encodes them once
/// so Python builds its own Arrow table, and the terminal is handed over as
/// JSON so the wire contract stays the only terminal shape.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "QueryResult")]
pub struct PyQueryResult {
    /// The native result every projection reads.
    inner: crate::QueryResult,
}

#[pymethods]
impl PyQueryResult {
    /// Encodes the whole result as one Arrow IPC stream.
    ///
    /// # Errors
    ///
    /// Raises a typed Bifrost query error when IPC encoding fails.
    fn to_ipc(&self) -> PyResult<Vec<u8>> {
        self.inner.to_ipc().map_err(query_error_to_py)
    }

    /// The validated terminal frame, serialized.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` when the terminal cannot be serialized.
    #[getter]
    fn terminal_json(&self) -> PyResult<String> {
        serde_json::to_string(self.inner.terminal())
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))
    }

    /// Total decoded rows across every batch.
    fn __len__(&self) -> usize {
        self.inner.num_rows()
    }
}

/// Python-facing [`crate::Bifrost`]: query any authorized table, write to the
/// active one.
///
/// Construction dials the ingest channel, so it performs IO; every transport
/// argument is optional and falls through the existing resolution chain exactly
/// once when omitted.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "Bifrost")]
pub struct Bifrost {
    /// The one native client both public Python facades drive.
    handle: crate::Bifrost,
}

#[pymethods]
impl Bifrost {
    /// Connects one client, optionally already bound to a write target.
    ///
    /// # Errors
    ///
    /// Raises [`NoCredentialsError`] when nothing in the chain resolves a
    /// credential, and a typed Bifrost query error when the ingest channel
    /// cannot be dialled.
    #[new]
    #[pyo3(signature = (table=None, server_url=None, credential=None, grpc_url=None))]
    fn __new__(
        py: Python<'_>,
        table: Option<PyTableConfig>,
        server_url: Option<&str>,
        credential: Option<&str>,
        grpc_url: Option<&str>,
    ) -> PyResult<Self> {
        let client =
            client_from_options(server_url, credential, grpc_url).map_err(query_error_to_py)?;
        let table = table.map(|table| table.inner);
        let handle = py
            .detach(|| {
                wyrd_runtime::runtime().block_on(crate::Bifrost::connect_with_config(
                    &client,
                    table,
                    wyrd_queue::QueueConfig::default(),
                ))
            })
            .map_err(query_error_to_py)?;
        Ok(Self { handle })
    }

    /// Creates the active table, answering `"created"` or `"already_exists"`.
    ///
    /// # Errors
    ///
    /// Raises a typed Bifrost query error when no table is bound, when a table
    /// of this name exists with different columns, or for transport and
    /// authorization failures.
    fn register(&self, py: Python<'_>) -> PyResult<&'static str> {
        let outcome = py
            .detach(|| wyrd_runtime::runtime().block_on(self.handle.register()))
            .map_err(query_error_to_py)?;
        Ok(crate::register_outcome_name(outcome))
    }

    /// Binds `table` as the write target, returning the previous binding.
    fn use_table(&self, table: PyTableConfig) -> Option<PyTableConfig> {
        self.handle
            .use_table(table.inner)
            .map(|inner| PyTableConfig { inner })
    }

    /// Binds an already-registered table by name, describing it first.
    ///
    /// # Errors
    ///
    /// As [`PyTableConfig::describe`].
    fn use_table_by_name(&self, py: Python<'_>, table: &str) -> PyResult<()> {
        py.detach(|| wyrd_runtime::runtime().block_on(self.handle.use_table_by_name(table)))
            .map_err(query_error_to_py)
    }

    /// The active write binding, if any.
    #[getter]
    fn table(&self) -> Option<PyTableConfig> {
        self.handle.table().map(|inner| PyTableConfig { inner })
    }

    /// Enqueues one JSON row into the active table.
    ///
    /// Non-blocking: the row is durable only after [`Bifrost::flush`] or
    /// [`Bifrost::shutdown`]. A saturated queue refuses here rather than
    /// dropping silently.
    ///
    /// # Errors
    ///
    /// Raises `ValueError` for an invalid card reference, and a typed Bifrost
    /// query error carrying `WYRD_VALA_412_NO_ACTIVE_TABLE` when no table is
    /// bound or `WYRD_CLIENT_429_QUEUE_FULL` when the producer is saturated.
    #[pyo3(signature = (row, card_ref=None, run_id=None))]
    fn insert(&self, row: &str, card_ref: Option<&str>, run_id: Option<String>) -> PyResult<()> {
        self.handle
            .insert(row.as_bytes().to_vec(), correlation(card_ref, run_id)?)
            .map_err(query_error_to_py)
    }

    /// Flushes every pooled producer and awaits each durable acknowledgement.
    ///
    /// # Errors
    ///
    /// Raises a typed Bifrost query error carrying the first producer or sink
    /// failure after every producer has been attempted.
    fn flush(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| wyrd_runtime::runtime().block_on(self.handle.flush()))
            .map_err(query_error_to_py)
    }

    /// Drains every producer and stops its background task.
    ///
    /// # Errors
    ///
    /// As [`Bifrost::flush`].
    fn shutdown(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| wyrd_runtime::runtime().block_on(self.handle.shutdown()))
            .map_err(query_error_to_py)
    }

    /// Runs one SQL SELECT and collects every batch.
    ///
    /// # Errors
    ///
    /// Raises [`IncompleteQueryStreamError`] when the response ends without its
    /// required terminal, and a typed Bifrost query error for invalid SQL, the
    /// query floor's refusal, or transport and authorization failures.
    fn sql(&self, py: Python<'_>, query: &str) -> PyResult<PyQueryResult> {
        let inner = py
            .detach(|| wyrd_runtime::runtime().block_on(self.handle.sql(query)))
            .map_err(query_error_to_py)?;
        Ok(PyQueryResult { inner })
    }

    /// Starts one query and returns its terminal-validating native stream.
    ///
    /// # Errors
    ///
    /// Raises `ValueError` for an unknown visibility or freshness spelling, and
    /// a typed Bifrost query error for request-contract or transport failures.
    #[pyo3(signature = (sql, visibility="published_only", freshness="strict", deadline_ms=None))]
    fn stream(
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
            .detach(|| wyrd_runtime::runtime().block_on(self.handle.query_client().query(&request)))
            .map_err(query_error_to_py)?;
        let request_id = stream.request_id().as_str().to_owned();
        Ok(PyBifrostQueryStream {
            request_id,
            consumer: Mutex::new(()),
            stream: Mutex::new(Some(NativeStreamOwner::Production(Box::new(stream)))),
            poll_abort: Mutex::new(PollAbortState::new()),
            terminal_json: Mutex::new(None),
        })
    }

    /// Lists active queries for this client's authenticated tenant.
    ///
    /// # Errors
    ///
    /// Raises a typed Bifrost query error for transport or server failures.
    fn running(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let queries = py
            .detach(|| wyrd_runtime::runtime().block_on(self.query_client().running()))
            .map_err(query_error_to_py)?;
        to_python_json(py, serde_json::to_value(queries))
    }

    /// Gets one active query by its canonical request ID.
    ///
    /// # Errors
    ///
    /// Raises a typed Bifrost query error for malformed IDs, transport, or
    /// server failures.
    fn status(&self, py: Python<'_>, request_id: &str) -> PyResult<Py<PyAny>> {
        let request_id = parse_query_request_id(request_id).map_err(query_error_to_py)?;
        let summary = py
            .detach(|| wyrd_runtime::runtime().block_on(self.query_client().status(&request_id)))
            .map_err(query_error_to_py)?;
        to_python_json(py, serde_json::to_value(summary))
    }

    /// Requests server-side cancellation without closing a local stream.
    ///
    /// # Errors
    ///
    /// As [`Bifrost::status`].
    fn cancel(&self, py: Python<'_>, request_id: &str) -> PyResult<Py<PyAny>> {
        let request_id = parse_query_request_id(request_id).map_err(query_error_to_py)?;
        let response = py
            .detach(|| wyrd_runtime::runtime().block_on(self.query_client().cancel(&request_id)))
            .map_err(query_error_to_py)?;
        to_python_json(py, serde_json::to_value(response))
    }

    /// Describes one registered table's stored physical schema.
    ///
    /// # Errors
    ///
    /// Raises a typed Bifrost query error for transport, authorization, or
    /// not-found failures.
    fn describe_table(&self, py: Python<'_>, namespace: &str, name: &str) -> PyResult<Py<PyAny>> {
        let description = py
            .detach(|| {
                wyrd_runtime::runtime()
                    .block_on(self.query_client().describe_table(namespace, name))
            })
            .map_err(query_error_to_py)?;
        to_python_json(py, serde_json::to_value(description))
    }

    /// Reads one complete authorized cut of a single trace.
    ///
    /// # Errors
    ///
    /// Raises `ValueError` when a bound is not an RFC 3339 timestamp, and a
    /// typed Bifrost query error for an inverted window, transport,
    /// authorization, or not-found failures.
    #[pyo3(signature = (trace_id, since=None, until=None))]
    fn get_trace(
        &self,
        py: Python<'_>,
        trace_id: &str,
        since: Option<&str>,
        until: Option<&str>,
    ) -> PyResult<Py<PyAny>> {
        let request = GetTraceRequest {
            trace_id: trace_id.to_owned(),
            since: parse_window_bound(since, "since")?,
            until: parse_window_bound(until, "until")?,
        };
        let response = py
            .detach(|| wyrd_runtime::runtime().block_on(self.query_client().get_trace(&request)))
            .map_err(query_error_to_py)?;
        to_python_json(py, serde_json::to_value(response))
    }

    /// Reads one page of GenAI generation records.
    ///
    /// The request arrives as one serialized [`QueryGenAiRequest`] so the wire
    /// contract stays the single source of the filter set; the Python wrapper
    /// owns the keyword-argument ergonomics.
    ///
    /// # Errors
    ///
    /// Raises `ValueError` when `request_json` is not one GenAI query request,
    /// and a typed Bifrost query error for transport, authorization, or
    /// validation failures.
    #[pyo3(signature = (request_json))]
    fn query_genai(&self, py: Python<'_>, request_json: &str) -> PyResult<Py<PyAny>> {
        let request: QueryGenAiRequest = serde_json::from_str(request_json)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let response = py
            .detach(|| wyrd_runtime::runtime().block_on(self.query_client().query_genai(&request)))
            .map_err(query_error_to_py)?;
        to_python_json(py, serde_json::to_value(response))
    }

    /// Rows dropped by the fire-and-forget observe path under backpressure.
    ///
    /// Always zero for [`Bifrost::insert`], which refuses rather than drops.
    #[getter]
    fn dropped(&self) -> u64 {
        self.handle.dropped()
    }

    /// Number of distinct table producers currently pooled.
    #[getter]
    fn producer_count(&self) -> usize {
        self.handle.producer_count()
    }
}

impl Bifrost {
    /// The native query plane every lifecycle and typed read delegates to.
    ///
    /// Not a `#[pymethods]` entry: Python reaches these reads through the
    /// client's own methods rather than a second exported client object.
    fn query_client(&self) -> &QueryClient {
        self.handle.query_client()
    }
}

/// Applies one optional serialized physical layout to a config.
///
/// # Errors
///
/// Returns `ValueError` when the text is not one `PhysicalLayoutWire`.
fn apply_layout(
    config: crate::TableConfig,
    layout_json: Option<&str>,
) -> PyResult<crate::TableConfig> {
    match layout_json {
        None => Ok(config),
        Some(layout) => {
            let layout: PhysicalLayoutWire = serde_json::from_str(layout).map_err(|error| {
                PyValueError::new_err(format!("invalid physical layout: {error}"))
            })?;
            Ok(config.with_layout(layout))
        }
    }
}

/// Parses the optional per-row correlation at the Python boundary.
///
/// # Errors
///
/// Returns `ValueError` when `card_ref` is not one parsable Card reference.
fn correlation(card_ref: Option<&str>, run_id: Option<String>) -> PyResult<Correlation> {
    let card_ref = card_ref
        .map(str::parse::<CardRef>)
        .transpose()
        .map_err(|error| PyValueError::new_err(format!("invalid card_ref: {error}")))?;
    Ok(Correlation {
        card_ref,
        run_id: run_id.map(RunId::from_string),
    })
}

/// Decodes one schema-only Arrow IPC stream into a native schema.
///
/// # Errors
///
/// Returns `ValueError` when the bytes are not one Arrow IPC schema.
fn decode_schema_ipc(bytes: &[u8]) -> PyResult<arrow_schema::SchemaRef> {
    let reader = StreamReader::try_new(Cursor::new(bytes), None)
        .map_err(|error| PyValueError::new_err(format!("invalid Arrow IPC schema: {error}")))?;
    Ok(reader.schema())
}

/// Encodes one native schema as a schema-only Arrow IPC stream.
///
/// # Errors
///
/// Returns `RuntimeError` when the schema cannot be encoded.
fn encode_schema_ipc(schema: &arrow_schema::SchemaRef) -> PyResult<Vec<u8>> {
    let mut buffer = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buffer, schema)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        writer
            .finish()
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    }
    Ok(buffer)
}

/// Projects one already-serialized server response into plain Python data.
///
/// # Errors
///
/// Returns `RuntimeError` when the response cannot be serialized.
fn to_python_json(
    py: Python<'_>,
    value: serde_json::Result<serde_json::Value>,
) -> PyResult<Py<PyAny>> {
    let value = value.map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    json_to_pyobject(py, &value)
}

/// Parses one optional RFC 3339 window bound at the Python boundary.
///
/// # Errors
///
/// Returns `ValueError` naming the offending field when the text is not an
/// RFC 3339 timestamp.
fn parse_window_bound(
    value: Option<&str>,
    field: &str,
) -> PyResult<Option<chrono::DateTime<chrono::Utc>>> {
    value
        .map(|text| {
            text.parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|error| {
                    PyValueError::new_err(format!("{field} must be an RFC 3339 timestamp: {error}"))
                })
        })
        .transpose()
}

/// Parses one lifecycle request identity into the canonical structured error boundary.
///
/// # Errors
///
/// Returns the stable Wyrd validation error when `value` is not a valid request ID.
fn parse_query_request_id(value: &str) -> Result<RequestId, ValaSdkError> {
    RequestId::parse(value).map_err(|error| {
        ValaSdkError::Transport(WyrdError::Validation {
            message: "request_id is invalid".to_owned(),
            details: serde_json::json!({"field": "request_id", "reason": error.to_string()}),
        })
    })
}

/// Native terminal-validating stream used by Python's async iterator facade.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "_NativeBifrostQueryStream")]
pub struct PyBifrostQueryStream {
    /// Canonical request identity exposed before the first body poll.
    request_id: String,
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
    /// Returns the canonical server lifecycle request identity.
    #[getter]
    fn request_id(&self) -> &str {
        &self.request_id
    }

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
/// Names its own table and schema per call rather than using the client's
/// active binding: an instrumented process writes its signals alongside
/// whatever the application is writing, and must not disturb the table the
/// application has bound. Queue saturation is swallowed and counted on the
/// client's drop counter instead of being raised.
///
/// # Errors
///
/// Raises `ValueError` for a malformed JSON schema, an unsupported schema node,
/// or an invalid card reference.
#[pyfunction]
#[pyo3(signature = (bifrost, table, schema, row, card_ref=None, run_id=None))]
fn record(
    bifrost: PyRef<'_, Bifrost>,
    table: &str,
    schema: &str,
    row: &str,
    card_ref: Option<&str>,
    run_id: Option<String>,
) -> PyResult<()> {
    let schema_value: serde_json::Value = serde_json::from_str(schema)
        .map_err(|error| PyValueError::new_err(format!("invalid JSON schema: {error}")))?;
    let schema = wyrd_queue::json_schema_to_arrow(&schema_value)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    crate::observe::record(
        &bifrost.handle,
        table,
        &std::sync::Arc::new(schema),
        row.as_bytes().to_vec(),
        correlation(card_ref, run_id)?,
    );
    Ok(())
}

/// Register the one Bifrost client and its result types on the supplied module.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn register_bifrost(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Bifrost>()?;
    module.add_class::<PyTableConfig>()?;
    module.add_class::<PyQueryResult>()?;
    module.add_class::<PyBifrostQueryStream>()?;
    module.add(
        "BifrostQueryError",
        module.py().get_type::<BifrostQueryError>(),
    )?;
    module.add(
        "IncompleteQueryStreamError",
        module.py().get_type::<IncompleteQueryStreamError>(),
    )?;
    module.add(
        "NoCredentialsError",
        module.py().get_type::<NoCredentialsError>(),
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
/// The exception type is selected from the error's stable code, not its text, so
/// a caller catches `NoCredentialsError` for an unresolvable chain and
/// `IncompleteQueryStreamError` for a truncated response without string
/// matching.
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
            } else if error.code() == NO_CREDENTIALS_CODE {
                py.get_type::<NoCredentialsError>()
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
