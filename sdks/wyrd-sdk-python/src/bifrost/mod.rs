//! `PyO3` boundary for the one Bifrost client.
//!
//! Every method converts its Python inputs to native contract types at the edge
//! — a JSON-Schema document or Arrow IPC schema to [`TableConfig`], a
//! card-ref string to [`wyrd_spec::reference::CardRef`] — and then calls the
//! Rust-native client. No queue, schema-mapping, registration, or query logic
//! is re-implemented here; the pool key and [`wyrd_client::bifrost::ClientScope`] stay opaque,
//! and Python never names them.
//!
//! Every entry point is blocking and releases the GIL. The public package's
//! synchronous `Bifrost` calls them directly and its `AsyncBifrost` runs the
//! same calls through `asyncio.to_thread`, so the two facades share one
//! implementation rather than two.

use serde_json::Result as JsonResult;
use serde_json::Value;
use std::fmt::Display;
use std::io::Cursor;
use std::sync::Mutex;

use arrow::ipc::reader::StreamReader;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use arrow_schema::SchemaRef;
use futures_util::future::{AbortHandle, Abortable};
use pyo3::prelude::*;
use pyo3::types::PyModule;
use wyrd_client::bifrost::Bifrost as NativeBifrost;
use wyrd_client::bifrost::QueryResult;
use wyrd_client::bifrost::TableConfig;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, PhysicalLayoutWire, VisibilityMode,
};
use wyrd_spec::vala::ids::RunId;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult, json_to_pyobject};

use wyrd_client::bifrost::Correlation;
use wyrd_client::bifrost::{BifrostClientError, QueryResultStream, client_from_options};

/// Widens one Bifrost client failure into the shared Wyrd Python boundary error.
///
/// The catalog projection (`WyrdError::from`) stays the client crate's single
/// public mapping; this only routes it into `wyrd_utils`' sole final projector.
/// A free function because both types live in foreign crates.
fn client_error(error: BifrostClientError) -> WyrdPyError {
    WyrdPyError::from(WyrdError::from(error))
}

/// Builds one boundary validation failure for a rejected Python argument.
///
/// Every Wyrd-owned input rejection on this boundary is a catalog
/// `WYRD_SPEC_400_VALIDATION` naming the offending argument, so Python callers
/// branch on the stable code and read `details["field"]` instead of parsing a
/// `ValueError` message.
fn invalid_argument(field: &str, reason: impl Display) -> WyrdPyError {
    WyrdPyError::from(WyrdError::Validation {
        message: format!("{field} is invalid: {reason}"),
        details: serde_json::json!({ "field": field, "reason": reason.to_string() }),
    })
}

/// Sentinel the native poll uses to distinguish a missing terminal from a
/// terminal that failed to serialize.
///
/// Both arrive as the same `Err(String)` from the terminal projection, so the
/// poll compares against this marker to raise the stable incomplete-stream
/// code instead of an internal error.
const MISSING_TERMINAL: &str = "query stream ended before its required terminal frame";

/// Builds one boundary internal failure for a broken local invariant.
///
/// Poisoned synchronization and failed local encoding are neither a caller
/// mistake nor a server refusal, so they project as the catalog's internal
/// error rather than borrowing a 4xx code that would tell the caller to retry
/// differently.
fn boundary_internal(detail: impl Into<String>) -> WyrdPyError {
    WyrdPyError::from(WyrdError::Internal {
        message: detail.into(),
        details: serde_json::json!({ "boundary": "bifrost_python" }),
    })
}

/// Python-facing [`TableConfig`]: one Bifrost table's identity, declared
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
    inner: TableConfig,
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
    /// Raises `WyrdError` when the table is not `namespace.name`, the document
    /// is not one mappable JSON Schema, a declared column is server-owned, or
    /// the layout is not one physical-layout declaration.
    #[staticmethod]
    #[pyo3(signature = (table, schema_json, layout_json=None))]
    fn from_json_schema(
        table: &str,
        schema_json: &str,
        layout_json: Option<&str>,
    ) -> WyrdPyResult<Self> {
        let schema: Value = serde_json::from_str(schema_json)
            .map_err(|error| invalid_argument("schema_json", error))?;
        let config = TableConfig::from_json_schema(table, &schema).map_err(client_error)?;
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
    /// Raises `WyrdError` when the bytes are not one Arrow IPC schema, the
    /// table is not `namespace.name`, a declared column is server-owned, or the
    /// layout is not one physical-layout declaration.
    #[staticmethod]
    #[pyo3(signature = (table, schema_ipc, layout_json=None))]
    fn from_arrow_ipc(
        table: &str,
        schema_ipc: &[u8],
        layout_json: Option<&str>,
    ) -> WyrdPyResult<Self> {
        let schema = decode_schema_ipc(schema_ipc)?;
        let config = TableConfig::from_arrow(table, schema).map_err(client_error)?;
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
    /// Raises `WyrdError` carrying `WYRD_CLIENT_401_NO_CREDENTIALS` when
    /// nothing resolves a credential, and the catalog code for not-found,
    /// authorization, transport, or schema-projection failures.
    #[staticmethod]
    #[pyo3(signature = (table, server_url=None, credential=None, grpc_url=None))]
    fn describe(
        py: Python<'_>,
        table: &str,
        server_url: Option<&str>,
        credential: Option<&str>,
        grpc_url: Option<&str>,
    ) -> WyrdPyResult<Self> {
        let client = client_from_options(server_url, credential, grpc_url).map_err(client_error)?;
        let inner = py
            .detach(|| wyrd_runtime::runtime().block_on(TableConfig::describe(&client, table)))
            .map_err(client_error)?;
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
    /// Raises `WyrdError` when the schema cannot be encoded.
    #[getter]
    fn arrow_schema_ipc(&self) -> WyrdPyResult<Vec<u8>> {
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
    inner: QueryResult,
}

#[pymethods]
impl PyQueryResult {
    /// Encodes the whole result as one Arrow IPC stream.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` when IPC encoding fails.
    fn to_ipc(&self) -> WyrdPyResult<Vec<u8>> {
        self.inner.to_ipc().map_err(client_error)
    }

    /// The validated terminal frame, serialized.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` when the terminal cannot be serialized.
    #[getter]
    fn terminal_json(&self) -> WyrdPyResult<String> {
        serde_json::to_string(self.inner.terminal()).map_err(|error| {
            boundary_internal(format!("query terminal is not serializable: {error}"))
        })
    }

    /// Total decoded rows across every batch.
    fn __len__(&self) -> usize {
        self.inner.num_rows()
    }
}

/// Python-facing [`NativeBifrost`]: query any authorized table, write to the
/// active one.
///
/// Construction dials the ingest channel, so it performs IO; every transport
/// argument is optional and falls through the existing resolution chain exactly
/// once when omitted.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "Bifrost")]
pub struct Bifrost {
    /// The one native client both public Python facades drive.
    handle: NativeBifrost,
}

#[pymethods]
impl Bifrost {
    /// Connects one client, optionally already bound to a write target.
    ///
    /// Without `client`, the transport resolves from the explicit options and
    /// then the environment chain exactly as before. With `client`, the
    /// supplied Rust `WyrdClient` — plain or delegated — is used as is, so no
    /// second credential is resolved; it cannot be combined with
    /// `server_url`, `credential`, or `grpc_url`.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` carrying `WYRD_SPEC_400_VALIDATION` when `client` is
    /// combined with a transport option, `WYRD_CLIENT_401_NO_CREDENTIALS` when
    /// nothing in the chain resolves a credential, and the catalog code for a
    /// failure to dial the ingest channel.
    #[new]
    #[pyo3(signature = (table=None, server_url=None, credential=None, grpc_url=None, client=None))]
    fn __new__(
        py: Python<'_>,
        table: Option<PyTableConfig>,
        server_url: Option<&str>,
        credential: Option<&str>,
        grpc_url: Option<&str>,
        client: Option<PyRef<'_, crate::client::PyWyrdClient>>,
    ) -> WyrdPyResult<Self> {
        let client = match client {
            Some(_) if server_url.is_some() || credential.is_some() || grpc_url.is_some() => {
                return Err(invalid_argument(
                    "client",
                    "cannot be combined with server_url, credential, or grpc_url",
                ));
            }
            Some(client) => client.inner().clone(),
            None => client_from_options(server_url, credential, grpc_url).map_err(client_error)?,
        };
        let table = table.map(|table| table.inner);
        let handle = py
            .detach(|| {
                wyrd_runtime::runtime().block_on(NativeBifrost::connect_with_config(
                    &client,
                    table,
                    wyrd_queue::QueueConfig::default(),
                ))
            })
            .map_err(client_error)?;
        Ok(Self { handle })
    }

    /// Creates the active table, answering `"created"` or `"already_exists"`.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` when no table is bound, when a table of this name
    /// exists with different columns, or for transport and authorization
    /// failures.
    fn register(&self, py: Python<'_>) -> WyrdPyResult<&'static str> {
        let outcome = py
            .detach(|| wyrd_runtime::runtime().block_on(self.handle.register()))
            .map_err(client_error)?;
        Ok(wyrd_client::bifrost::register_outcome_name(outcome))
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
    fn use_table_by_name(&self, py: Python<'_>, table: &str) -> WyrdPyResult<()> {
        py.detach(|| wyrd_runtime::runtime().block_on(self.handle.use_table_by_name(table)))
            .map_err(client_error)?;
        Ok(())
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
    /// Raises `WyrdError` carrying `WYRD_SPEC_400_VALIDATION` for an invalid
    /// card reference, `WYRD_VALA_412_NO_ACTIVE_TABLE` when no table is bound,
    /// or `WYRD_CLIENT_429_QUEUE_FULL` when the producer is saturated.
    #[pyo3(signature = (row, card_ref=None, run_id=None))]
    fn insert(
        &self,
        row: &str,
        card_ref: Option<&str>,
        run_id: Option<String>,
    ) -> WyrdPyResult<()> {
        self.handle
            .insert(row.as_bytes().to_vec(), correlation(card_ref, run_id)?)
            .map_err(client_error)?;
        Ok(())
    }

    /// Writes one already-built Arrow batch to `table` and awaits durability.
    ///
    /// The batch arrives as an Arrow IPC stream because that is the boundary
    /// `pyarrow` and this crate already share; the Python wrapper encodes the
    /// caller's `pyarrow.RecordBatch` and this decodes it once. Unlike
    /// [`PyBifrost::insert`] the batch is not buffered, so no later flush is
    /// needed, and the destination is named rather than taken from the active
    /// binding.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` when the bytes are not one Arrow IPC batch, and the
    /// catalog code for the encode, byte-envelope, or stable server refusal
    /// reported by the write.
    #[pyo3(signature = (table, batch_ipc))]
    fn write_batch(&self, py: Python<'_>, table: &str, batch_ipc: &[u8]) -> WyrdPyResult<()> {
        let batch = decode_batch_ipc(batch_ipc)?;
        py.detach(|| wyrd_runtime::runtime().block_on(self.handle.write_batch(table, &batch)))
            .map_err(client_error)?;
        Ok(())
    }

    /// Flushes every pooled producer and awaits each durable acknowledgement.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` carrying the first producer or sink failure after
    /// every producer has been attempted.
    fn flush(&self, py: Python<'_>) -> WyrdPyResult<()> {
        py.detach(|| wyrd_runtime::runtime().block_on(self.handle.flush()))
            .map_err(client_error)?;
        Ok(())
    }

    /// Drains every producer and stops its background task.
    ///
    /// # Errors
    ///
    /// As [`Bifrost::flush`].
    fn shutdown(&self, py: Python<'_>) -> WyrdPyResult<()> {
        py.detach(|| wyrd_runtime::runtime().block_on(self.handle.shutdown()))
            .map_err(client_error)?;
        Ok(())
    }

    /// Runs one SQL SELECT and collects every batch.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` carrying `WYRD_VALA_502_QUERY_STREAM_INCOMPLETE`
    /// when the response ends without its required terminal, and the catalog
    /// code for invalid SQL, the query floor's refusal, or transport and
    /// authorization failures.
    fn sql(&self, py: Python<'_>, query: &str) -> WyrdPyResult<PyQueryResult> {
        let inner = py
            .detach(|| wyrd_runtime::runtime().block_on(self.handle.sql(query)))
            .map_err(client_error)?;
        Ok(PyQueryResult { inner })
    }

    /// Starts one query and returns its terminal-validating native stream.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for an unknown visibility or freshness spelling, and
    /// the catalog code for request-contract or transport failures.
    #[pyo3(signature = (sql, visibility="published_only", freshness="strict", deadline_ms=None))]
    fn stream(
        &self,
        py: Python<'_>,
        sql: &str,
        visibility: &str,
        freshness: &str,
        deadline_ms: Option<i64>,
    ) -> WyrdPyResult<PyBifrostQueryStream> {
        let request = BifrostQueryRequest {
            sql: sql.to_owned(),
            visibility: parse_visibility(visibility)?,
            freshness: parse_freshness(freshness)?,
            deadline_ms,
        };
        let stream = py
            .detach(|| wyrd_runtime::runtime().block_on(self.handle.query(&request)))
            .map_err(client_error)?;
        let request_id = stream.request_id().as_str().to_owned();
        Ok(PyBifrostQueryStream {
            request_id,
            consumer: Mutex::new(()),
            stream: Mutex::new(Some(Box::new(stream))),
            poll_abort: Mutex::new(PollAbortState::new()),
            terminal_json: Mutex::new(None),
        })
    }

    /// Lists active queries for this client's authenticated tenant.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for transport or server failures.
    fn running(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        let queries = py
            .detach(|| wyrd_runtime::runtime().block_on(self.handle.running()))
            .map_err(client_error)?;
        to_python_json(py, serde_json::to_value(queries))
    }

    /// Gets one active query by its canonical request ID.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for malformed IDs, transport, or server failures.
    fn status(&self, py: Python<'_>, request_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let request_id = parse_query_request_id(request_id).map_err(client_error)?;
        let summary = py
            .detach(|| wyrd_runtime::runtime().block_on(self.handle.status(&request_id)))
            .map_err(client_error)?;
        to_python_json(py, serde_json::to_value(summary))
    }

    /// Requests server-side cancellation without closing a local stream.
    ///
    /// # Errors
    ///
    /// As [`Bifrost::status`].
    fn cancel(&self, py: Python<'_>, request_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let request_id = parse_query_request_id(request_id).map_err(client_error)?;
        let response = py
            .detach(|| wyrd_runtime::runtime().block_on(self.handle.cancel(&request_id)))
            .map_err(client_error)?;
        to_python_json(py, serde_json::to_value(response))
    }

    /// Describes one registered table's stored physical schema.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for transport, authorization, or not-found failures.
    fn describe_table(
        &self,
        py: Python<'_>,
        namespace: &str,
        name: &str,
    ) -> WyrdPyResult<Py<PyAny>> {
        let description = py
            .detach(|| {
                wyrd_runtime::runtime()
                    .block_on(self.handle.describe(&format!("{namespace}.{name}")))
            })
            .map_err(client_error)?;
        to_python_json(py, serde_json::to_value(description))
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

/// Applies one optional serialized physical layout to a config.
///
/// # Errors
///
/// Returns the stable Wyrd validation error when the text is not one
/// `PhysicalLayoutWire`.
fn apply_layout(config: TableConfig, layout_json: Option<&str>) -> WyrdPyResult<TableConfig> {
    match layout_json {
        None => Ok(config),
        Some(layout) => {
            let layout: PhysicalLayoutWire = serde_json::from_str(layout)
                .map_err(|error| invalid_argument("layout_json", error))?;
            Ok(config.with_layout(layout))
        }
    }
}

/// Parses the optional per-row correlation at the Python boundary.
///
/// # Errors
///
/// Returns the stable Wyrd validation error when `card_ref` is not one
/// parsable Card reference.
fn correlation(card_ref: Option<&str>, run_id: Option<String>) -> WyrdPyResult<Correlation> {
    let card_ref = card_ref
        .map(str::parse::<CardRef>)
        .transpose()
        .map_err(|error| invalid_argument("card_ref", error))?;
    Ok(Correlation {
        card_ref,
        run_id: run_id.map(RunId::from_string),
    })
}

/// Decodes one schema-only Arrow IPC stream into a native schema.
///
/// # Errors
///
/// Returns the stable Wyrd validation error when the bytes are not one Arrow
/// IPC schema.
fn decode_schema_ipc(bytes: &[u8]) -> WyrdPyResult<SchemaRef> {
    let reader = StreamReader::try_new(Cursor::new(bytes), None)
        .map_err(|error| invalid_argument("schema_ipc", error))?;
    Ok(reader.schema())
}

/// Decodes one single-batch Arrow IPC stream into a native record batch.
///
/// # Errors
///
/// Returns the stable Wyrd validation error when the bytes are not one Arrow
/// IPC stream carrying exactly one batch, which is what one logical write is.
fn decode_batch_ipc(bytes: &[u8]) -> WyrdPyResult<RecordBatch> {
    let mut reader = StreamReader::try_new(Cursor::new(bytes), None)
        .map_err(|error| invalid_argument("batch_ipc", error))?;
    let batch = reader
        .next()
        .ok_or_else(|| invalid_argument("batch_ipc", "Arrow IPC stream carries no batch"))?
        .map_err(|error| invalid_argument("batch_ipc", error))?;
    if reader.next().is_some() {
        return Err(invalid_argument(
            "batch_ipc",
            "one write carries exactly one Arrow batch",
        ));
    }
    Ok(batch)
}

/// Encodes one native schema as a schema-only Arrow IPC stream.
///
/// # Errors
///
/// Returns the stable Wyrd internal error when the schema cannot be encoded.
fn encode_schema_ipc(schema: &SchemaRef) -> WyrdPyResult<Vec<u8>> {
    let mut buffer = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buffer, schema)
            .map_err(|error| boundary_internal(error.to_string()))?;
        writer
            .finish()
            .map_err(|error| boundary_internal(error.to_string()))?;
    }
    Ok(buffer)
}

/// Projects one already-serialized server response into plain Python data.
///
/// # Errors
///
/// Returns the stable Wyrd internal error when the response cannot be
/// serialized, and the intrinsic `PyO3` error when Python object construction
/// fails.
fn to_python_json(py: Python<'_>, value: JsonResult<Value>) -> WyrdPyResult<Py<PyAny>> {
    let value = value.map_err(|error| boundary_internal(error.to_string()))?;
    json_to_pyobject(py, &value).map_err(|error| boundary_internal(error.to_string()))
}

/// Parses one lifecycle request identity into the canonical structured error boundary.
///
/// # Errors
///
/// Returns the stable Wyrd validation error when `value` is not a valid request ID.
fn parse_query_request_id(value: &str) -> Result<RequestId, BifrostClientError> {
    RequestId::parse(value).map_err(|error| {
        BifrostClientError::Transport(WyrdError::Validation {
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
    stream: Mutex<Option<Box<QueryResultStream>>>,
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
        error: BifrostClientError,
        /// Optional serialized terminal retained before projecting the error.
        terminal: Option<String>,
    },
    /// Local serialization or Arrow encoding failed.
    ProjectionError(String),
}

impl PyBifrostQueryStream {
    /// Retains one serialized terminal so `terminal_json` can return it after the poll.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` when the terminal lock is poisoned.
    fn retain_terminal(&self, terminal: String) -> WyrdPyResult<()> {
        *self
            .terminal_json
            .lock()
            .map_err(|_| boundary_internal("terminal lock poisoned"))? = Some(terminal);
        Ok(())
    }
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
    /// Raises `WyrdError` for protocol, terminal, transport, Arrow, or
    /// poisoned-state failures.
    fn next_ipc(&self, py: Python<'_>) -> WyrdPyResult<Option<Vec<u8>>> {
        py.detach(|| {
            let _consumer = self
                .consumer
                .lock()
                .map_err(|_| boundary_internal("query consumer lock poisoned"))?;
            let (abort_handle, abort_registration) = AbortHandle::new_pair();
            let poll_id = self
                .poll_abort
                .lock()
                .map_err(|_| boundary_internal("query abort lock poisoned"))?
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
                                        terminal.ok_or_else(|| MISSING_TERMINAL.to_owned())
                                    });
                                drop(stream_slot.take());
                                match terminal {
                                    Ok(terminal) => NativePollOutcome::Complete(terminal),
                                    Err(error) if error == MISSING_TERMINAL => {
                                        NativePollOutcome::QueryError {
                                            error: BifrostClientError::IncompleteQueryStream,
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
                .map_err(|_| boundary_internal("query abort lock poisoned"))?
                .clear(poll_id);

            match outcome {
                NativePollOutcome::Batch(batch) => Ok(Some(batch)),
                NativePollOutcome::Complete(terminal) => {
                    self.retain_terminal(terminal)?;
                    Ok(None)
                }
                NativePollOutcome::Closed => {
                    Err(invalid_argument("stream", "query stream is closed"))
                }
                NativePollOutcome::Aborted => Err(client_error(BifrostClientError::Protocol(
                    "query stream poll aborted".to_owned(),
                ))),
                NativePollOutcome::QueryError { error, terminal } => {
                    if let Some(terminal) = terminal {
                        self.retain_terminal(terminal)?;
                    }
                    Err(client_error(error))
                }
                NativePollOutcome::ProjectionError(error) => Err(boundary_internal(error)),
            }
        })
    }

    /// Returns serialized terminal metadata after validated completion.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` when internal synchronization was poisoned.
    #[getter]
    fn terminal_json(&self) -> WyrdPyResult<Option<String>> {
        self.terminal_json
            .lock()
            .map(|terminal| terminal.clone())
            .map_err(|_| boundary_internal("terminal lock poisoned"))
    }

    /// Explicitly drops the Rust response stream to propagate cancellation.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` when internal synchronization was poisoned.
    fn close(&self) -> WyrdPyResult<()> {
        self.poll_abort
            .lock()
            .map_err(|_| boundary_internal("query abort lock poisoned"))?
            .abort_current();
        drop(
            self.stream
                .lock()
                .map_err(|_| boundary_internal("query stream lock poisoned"))?
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
    /// Always raises `WyrdError` carrying
    /// `WYRD_VALA_502_QUERY_STREAM_INCOMPLETE`.
    #[staticmethod]
    fn raise_incomplete_error() -> WyrdPyResult<()> {
        Err(client_error(BifrostClientError::IncompleteQueryStream))
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
/// Raises `WyrdError` carrying `WYRD_SPEC_400_VALIDATION` for a malformed JSON
/// schema, an unsupported schema node, or an invalid card reference.
#[pyfunction]
#[pyo3(signature = (bifrost, table, schema, row, card_ref=None, run_id=None))]
fn record(
    bifrost: &Bound<'_, Bifrost>,
    table: &str,
    schema: &str,
    row: &str,
    card_ref: Option<&str>,
    run_id: Option<String>,
) -> WyrdPyResult<()> {
    let schema_value: Value =
        serde_json::from_str(schema).map_err(|error| invalid_argument("schema", error))?;
    let schema = wyrd_queue::json_schema_to_arrow(&schema_value)
        .map_err(|error| invalid_argument("schema", error))?;
    wyrd_client::bifrost::observe::record(
        &bifrost.borrow().handle,
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
/// Returns `PyO3` registration errors.
pub fn register_bifrost(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Bifrost>()?;
    module.add_class::<PyTableConfig>()?;
    module.add_class::<PyQueryResult>()?;
    module.add_class::<PyBifrostQueryStream>()?;
    Ok(())
}

/// Register the `observe` fire-and-forget telemetry function on the module.
///
/// # Errors
/// Returns `PyO3` registration errors.
pub fn register_observe(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(record, module)?)?;
    Ok(())
}

/// Parses the Python visibility spelling into the closed Rust contract.
///
/// # Errors
///
/// Raises the stable Wyrd validation error for any value other than
/// `published_only`, `published-only`, or `fused`.
fn parse_visibility(value: &str) -> WyrdPyResult<VisibilityMode> {
    match value {
        "published_only" | "published-only" => Ok(VisibilityMode::PublishedOnly),
        "fused" => Ok(VisibilityMode::Fused),
        _ => Err(invalid_argument(
            "visibility",
            "must be 'published_only' or 'fused'",
        )),
    }
}

/// Parses the Python freshness spelling into the closed Rust contract.
///
/// # Errors
///
/// Raises the stable Wyrd validation error for any value other than `strict`,
/// `allow_degraded`, or `allow-degraded`.
fn parse_freshness(value: &str) -> WyrdPyResult<FreshnessPolicy> {
    match value {
        "strict" => Ok(FreshnessPolicy::Strict),
        "allow_degraded" | "allow-degraded" => Ok(FreshnessPolicy::AllowDegraded),
        _ => Err(invalid_argument(
            "freshness",
            "must be 'strict' or 'allow_degraded'",
        )),
    }
}

/// Encodes one Rust record batch for the public Python `PyArrow` conversion.
///
/// # Errors
///
/// Returns the encoder's own message when Arrow IPC encoding fails; the caller
/// projects it onto the catalog.
fn encode_batch(batch: &RecordBatch) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, batch.schema().as_ref())
        .map_err(|error| error.to_string())?;
    writer
        .write(batch)
        .and_then(|()| writer.finish())
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}
