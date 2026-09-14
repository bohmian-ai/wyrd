//! Thin napi projection of the Rust-owned `wyrd_client` capabilities:
//! Bifrost, Cards, and offline `WyrdState`.

#![deny(missing_docs)]

pub mod cards;

use std::sync::{Arc, Mutex};

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use napi::Error;
use napi::Result as NapiResult;
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use serde::Serialize;
use serde_json::Value;
use std::fmt::Display;
use tokio::sync::Mutex as AsyncMutex;
use wyrd_client::bifrost::Bifrost;
use wyrd_client::bifrost::Correlation;
use wyrd_client::bifrost::TableConfig;
use wyrd_client::bifrost::{BifrostClientError, QueryResultStream};
use wyrd_queue::QueueConfig;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_spec::vala::error::BifrostError;
use wyrd_spec::vala::ids::RunId;

/// JavaScript query request projected onto the pure Wyrd contract.
#[napi(object)]
pub struct NativeQueryRequest {
    /// SELECT-only SQL text.
    pub sql: String,
    /// `published_only` or `fused`.
    pub visibility: String,
    /// `strict` or `allow_degraded`.
    pub freshness: String,
    /// Optional query deadline in milliseconds, valid in `1..=u32::MAX`.
    ///
    /// Accepted as a JavaScript number so every out-of-range, fractional, or
    /// non-finite value reaches the structured startup failure instead of a
    /// napi binding error.
    pub deadline_ms: Option<f64>,
}

/// One raw native iterator step consumed by the TypeScript Arrow facade.
#[napi(object)]
pub struct NativeQueryStep {
    /// Complete Arrow IPC stream containing exactly one record batch.
    pub ipc: Option<Buffer>,
    /// Serialized validated terminal, present only on the final step.
    pub terminal_json: Option<String>,
    /// Stable SDK error code for a failed native step.
    pub error_code: Option<String>,
    /// HTTP-equivalent status for a failed native step.
    pub error_status: Option<u32>,
    /// Stable title for a failed native step.
    pub error_title: Option<String>,
    /// Scrubbed SDK error detail for a failed native step.
    pub error_detail: Option<String>,
    /// Operator-facing remediation for a failed native step.
    pub error_remediation: Option<String>,
    /// Serialized JSON-safe structured details for a failed native step.
    pub error_details_json: Option<String>,
}

/// Structured native result for one live-query lifecycle control.
#[napi(object)]
pub struct NativeLifecycleResult {
    /// Canonical JSON payload when the control succeeded.
    pub value_json: Option<String>,
    /// Stable SDK error code when the control failed.
    pub error_code: Option<String>,
    /// HTTP-equivalent status when the control failed.
    pub error_status: Option<u32>,
    /// Stable title when the control failed.
    pub error_title: Option<String>,
    /// Scrubbed detail when the control failed.
    pub error_detail: Option<String>,
    /// Operator-facing remediation when the control failed.
    pub error_remediation: Option<String>,
    /// Serialized JSON-safe structured details when the control failed.
    pub error_details_json: Option<String>,
}

impl NativeLifecycleResult {
    /// Builds one successful JSON lifecycle projection.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the canonical lifecycle value cannot be serialized.
    fn success(value: &Value) -> NapiResult<Self> {
        Ok(Self {
            value_json: Some(serde_json::to_string(&value).map_err(napi_error)?),
            error_code: None,
            error_status: None,
            error_title: None,
            error_detail: None,
            error_remediation: None,
            error_details_json: None,
        })
    }

    /// Projects one Rust-owned result as its JSON value or catalog-backed failure.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the successful value cannot be serialized.
    fn outcome<T: Serialize>(result: Result<T, WyrdError>) -> NapiResult<Self> {
        match result {
            Ok(value) => Self::success(&serde_json::to_value(value).map_err(napi_error)?),
            Err(error) => Ok(Self::from_wyrd(&error)),
        }
    }

    /// Builds one failed lifecycle projection with stable Wyrd metadata.
    fn failure(error: &BifrostClientError) -> Self {
        Self::from_wyrd(&WyrdError::from(error))
    }

    /// Builds one failed projection directly from a catalog error.
    fn from_wyrd(projected: &WyrdError) -> Self {
        let error = NativeWyrdError::from_wyrd(projected);
        Self {
            value_json: None,
            error_code: Some(error.error_code),
            error_status: Some(error.error_status),
            error_title: Some(error.error_title),
            error_detail: Some(error.error_detail),
            error_remediation: Some(error.error_remediation),
            error_details_json: error.error_details_json,
        }
    }
}

/// Catalog metadata for one failed native construction or description.
///
/// Field names match the other native results so the TypeScript facade builds
/// its public `WyrdError` through the same projection.
#[napi(object)]
pub struct NativeWyrdError {
    /// Stable catalog code.
    pub error_code: String,
    /// HTTP-equivalent status.
    pub error_status: u32,
    /// Stable catalog title.
    pub error_title: String,
    /// Scrubbed catalog detail.
    pub error_detail: String,
    /// Operator-facing remediation.
    pub error_remediation: String,
    /// Serialized JSON-safe structured details, when present.
    pub error_details_json: Option<String>,
}

impl NativeWyrdError {
    /// Projects one catalog error onto its independent metadata fields.
    ///
    /// Public text comes from the problem document so it matches the HTTP and
    /// Python projections of the same error.
    fn from_wyrd(projected: &WyrdError) -> Self {
        let problem = projected.as_problem_json();
        Self {
            error_code: projected.code().to_owned(),
            error_status: u32::from(projected.status()),
            error_title: problem_field(&problem, "title", projected.title()),
            error_detail: problem_field(&problem, "detail", &projected.to_string()),
            error_remediation: projected.remediation().to_owned(),
            error_details_json: problem
                .get("details")
                .and_then(|value| serde_json::to_string(value).ok()),
        }
    }
}

/// Closed result of connecting one Bifrost client: a handle or a catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeBifrostConnection {
    /// Connected client when construction succeeded.
    pub bifrost: Option<NativeBifrost>,
    /// Catalog failure when construction failed.
    pub error: Option<NativeWyrdError>,
}

/// Closed result of describing one table: its config or a catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeTableConfigResult {
    /// Described table config when the server answered.
    pub config: Option<NativeTableConfig>,
    /// Catalog failure when description failed.
    pub error: Option<NativeWyrdError>,
}

/// Structured result of starting a native terminal-safe query.
#[napi]
pub struct NativeQueryStart {
    /// Rust-owned stream when startup succeeded.
    stream: Option<NativeBifrostQueryStream>,
    /// Stable SDK error code when startup failed.
    error_code: Option<String>,
    /// HTTP-equivalent status when startup failed.
    error_status: Option<u32>,
    /// Stable title when startup failed.
    error_title: Option<String>,
    /// Scrubbed SDK error detail when startup failed.
    error_detail: Option<String>,
    /// Operator-facing remediation when startup failed.
    error_remediation: Option<String>,
    /// Serialized JSON-safe structured details when startup failed.
    error_details_json: Option<String>,
}

impl NativeQueryStart {
    /// Builds the successful side of the closed startup result.
    fn success(stream: QueryResultStream) -> Self {
        Self {
            stream: Some(NativeBifrostQueryStream::new(stream)),
            error_code: None,
            error_status: None,
            error_title: None,
            error_detail: None,
            error_remediation: None,
            error_details_json: None,
        }
    }

    /// Builds the failed side with stable metadata retained as independent fields.
    fn failure(error: &BifrostClientError) -> Self {
        let projected = wyrd_spec::error::WyrdError::from(error);
        let problem = projected.as_problem_json();
        Self {
            stream: None,
            error_code: Some(projected.code().to_owned()),
            error_status: Some(u32::from(projected.status())),
            error_title: Some(problem_field(&problem, "title", projected.title())),
            error_detail: Some(problem_field(&problem, "detail", &projected.to_string())),
            error_remediation: Some(projected.remediation().to_owned()),
            error_details_json: problem
                .get("details")
                .and_then(|value| serde_json::to_string(value).ok()),
        }
    }
}

#[napi]
impl NativeQueryStart {
    /// Moves the Rust-owned stream out after a successful startup.
    #[napi]
    pub fn take_stream(&mut self) -> Option<NativeBifrostQueryStream> {
        self.stream.take()
    }

    /// Returns the stable SDK error code when startup failed.
    #[napi(getter)]
    pub fn error_code(&self) -> Option<String> {
        self.error_code.clone()
    }

    /// Returns the HTTP-equivalent status when startup failed.
    #[napi(getter)]
    pub fn error_status(&self) -> Option<u32> {
        self.error_status
    }

    /// Returns the stable title when startup failed.
    #[napi(getter)]
    pub fn error_title(&self) -> Option<String> {
        self.error_title.clone()
    }

    /// Returns the scrubbed SDK error detail when startup failed.
    #[napi(getter)]
    pub fn error_detail(&self) -> Option<String> {
        self.error_detail.clone()
    }

    /// Returns operator-facing remediation when startup failed.
    #[napi(getter)]
    pub fn error_remediation(&self) -> Option<String> {
        self.error_remediation.clone()
    }

    /// Returns serialized JSON-safe structured details when startup failed.
    #[napi(getter)]
    pub fn error_details_json(&self) -> Option<String> {
        self.error_details_json.clone()
    }
}

/// One Bifrost table as it crosses the Node boundary.
///
/// `configJson` is the authoritative value — the whole `TableConfig`, including
/// the server identity a described table already carries — so a config that
/// goes out to JavaScript and comes back is the same config. The other fields
/// are read-only projections JavaScript would otherwise have to derive, and
/// deriving them would mean reimplementing the schema mapping this SDK exists
/// to keep in one place.
#[napi(object)]
pub struct NativeTableConfig {
    /// The serialized `TableConfig`; the only field read back natively.
    pub config_json: String,
    /// `namespace.name`.
    pub fqn: String,
    /// The declared user columns as one schema-only Arrow IPC stream.
    pub schema_ipc: Buffer,
    /// Serialized `{table_uid, fingerprint}` once the server has minted it.
    pub resolved_json: Option<String>,
}

impl NativeTableConfig {
    /// Projects one native config for JavaScript.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the config or its schema cannot be encoded.
    fn project(config: &TableConfig) -> NapiResult<Self> {
        let mut schema_ipc = Vec::new();
        {
            let mut writer =
                arrow::ipc::writer::StreamWriter::try_new(&mut schema_ipc, config.user_schema())
                    .map_err(napi_error)?;
            writer.finish().map_err(napi_error)?;
        }
        Ok(Self {
            config_json: serde_json::to_string(config).map_err(napi_error)?,
            fqn: config.fqn(),
            schema_ipc: Buffer::from(schema_ipc),
            resolved_json: config
                .resolved()
                .map(serde_json::to_string)
                .transpose()
                .map_err(napi_error)?,
        })
    }

    /// Rebuilds the native config from the value JavaScript handed back.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the text is not one serialized `TableConfig`.
    fn parse(&self) -> NapiResult<TableConfig> {
        serde_json::from_str(&self.config_json).map_err(napi_error)
    }
}

/// Decodes one single-batch Arrow IPC stream into a native record batch.
///
/// # Errors
///
/// Returns a napi error when the bytes are not one Arrow IPC stream carrying
/// exactly one batch, which is what one logical write is.
fn decode_batch_ipc(bytes: &[u8]) -> NapiResult<RecordBatch> {
    let mut reader = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)
        .map_err(napi_error)?;
    let batch = reader
        .next()
        .ok_or_else(|| napi::Error::from_reason("Arrow IPC stream carries no batch"))?
        .map_err(napi_error)?;
    if reader.next().is_some() {
        return Err(napi::Error::from_reason(
            "one write carries exactly one Arrow batch",
        ));
    }
    Ok(batch)
}

/// Builds one table config from a JSON Schema document.
///
/// This is the Zod (`z.toJSONSchema()`) door; the mapping is the same
/// `wyrd-queue` owner every language uses, so one model declares the same
/// columns from any SDK.
///
/// # Errors
///
/// Returns a napi error when the table is not `namespace.name`, the document is
/// not one mappable JSON Schema, a declared column is server-owned, or the
/// layout is not one physical-layout declaration.
// justification: napi boundary; a JavaScript string is primitive and cannot be
// passed by reference, so the generated binding requires an owned String
#[allow(clippy::needless_pass_by_value)]
#[napi]
pub fn table_config_from_json_schema(
    table: String,
    schema_json: String,
    layout_json: Option<String>,
) -> NapiResult<NativeTableConfig> {
    let schema: Value = serde_json::from_str(&schema_json)
        .map_err(|error| napi::Error::from_reason(format!("invalid JSON schema: {error}")))?;
    let config = TableConfig::from_json_schema(&table, &schema).map_err(napi_error)?;
    NativeTableConfig::project(&apply_layout(config, layout_json.as_deref())?)
}

/// Fetches an already-registered table's config by name.
///
/// Every transport argument is optional and resolves through the same chain the
/// client constructor uses when omitted.
///
/// # Errors
///
/// Returns a napi error only when the described config cannot be encoded;
/// credential, transport, and server refusals are returned as catalog metadata.
// justification: napi boundary; a JavaScript string is primitive and cannot be
// passed by reference, so the generated binding requires an owned String
#[allow(clippy::needless_pass_by_value)]
#[napi]
pub async fn describe_table_config(
    table: String,
    server_url: Option<String>,
    credential: Option<String>,
    grpc_url: Option<String>,
) -> NapiResult<NativeTableConfigResult> {
    let described = match wyrd_client::bifrost::client_from_options(
        server_url.as_deref(),
        credential.as_deref(),
        grpc_url.as_deref(),
    ) {
        Ok(client) => TableConfig::describe(&client, &table).await,
        Err(error) => Err(error),
    };
    match described {
        Ok(config) => Ok(NativeTableConfigResult {
            config: Some(NativeTableConfig::project(&config)?),
            error: None,
        }),
        Err(error) => Ok(NativeTableConfigResult {
            config: None,
            error: Some(NativeWyrdError::from_wyrd(&WyrdError::from(&error))),
        }),
    }
}

/// Applies one optional serialized physical layout to a config.
///
/// # Errors
///
/// Returns a napi error when the text is not one `PhysicalLayoutWire`.
fn apply_layout(config: TableConfig, layout_json: Option<&str>) -> NapiResult<TableConfig> {
    match layout_json {
        None => Ok(config),
        Some(layout) => {
            let layout: PhysicalLayoutWire = serde_json::from_str(layout).map_err(|error| {
                napi::Error::from_reason(format!("invalid physical layout: {error}"))
            })?;
            Ok(config.with_layout(layout))
        }
    }
}

/// The one Bifrost client: query any authorized table, write to the active one.
///
/// Mirrors the Python binding: both are thin conversions over the one
/// [`Bifrost`], so batching, backpressure, registration, and the
/// query contract have exactly one owner.
#[napi]
pub struct NativeBifrost {
    /// The Rust-owned client every method delegates to.
    ///
    /// Shared through an [`Arc`] so a blocking drain can move it onto a
    /// blocking worker without stalling the Node event loop.
    client: Arc<Bifrost>,
}

/// Connects one Bifrost client, optionally already bound to a write target.
///
/// A free function rather than a constructor because connecting is asynchronous
/// and a napi constructor cannot be. Every transport argument is optional and
/// falls through the existing resolution chain exactly once when omitted.
///
/// # Errors
///
/// Returns a napi error only when the supplied table config is not one
/// serialized `TableConfig`; credential and ingest-dial failures are returned
/// as catalog metadata.
#[napi]
pub async fn connect_bifrost(
    table: Option<NativeTableConfig>,
    server_url: Option<String>,
    credential: Option<String>,
    grpc_url: Option<String>,
) -> NapiResult<NativeBifrostConnection> {
    let table = table.map(|table| table.parse()).transpose()?;
    let connected = match wyrd_client::bifrost::client_from_options(
        server_url.as_deref(),
        credential.as_deref(),
        grpc_url.as_deref(),
    ) {
        Ok(client) => Bifrost::connect_with_config(&client, table, QueueConfig::default()).await,
        Err(error) => Err(error),
    };
    Ok(match connected {
        Ok(handle) => NativeBifrostConnection {
            bifrost: Some(NativeBifrost {
                client: Arc::new(handle),
            }),
            error: None,
        },
        Err(error) => NativeBifrostConnection {
            bifrost: None,
            error: Some(NativeWyrdError::from_wyrd(&WyrdError::from(&error))),
        },
    })
}

#[napi]
impl NativeBifrost {
    /// Creates the active table, answering `created` or `already_exists`.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// no-active-table, fingerprint-conflict, and transport failures are
    /// returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn register(&self) -> NapiResult<NativeLifecycleResult> {
        match self.client.register().await {
            Ok(outcome) => NativeLifecycleResult::success(&serde_json::Value::String(
                wyrd_client::bifrost::register_outcome_name(outcome).to_owned(),
            )),
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Binds `table` as the write target, returning the previous binding.
    ///
    /// The previous table's producer stays pooled, so its buffered rows still
    /// flush; a swap loses nothing.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the config is not one serialized `TableConfig`
    /// or the previous binding cannot be projected.
    // justification: napi boundary; a generated object argument arrives owned
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub fn use_table(&self, table: NativeTableConfig) -> NapiResult<Option<NativeTableConfig>> {
        self.client
            .use_table(table.parse()?)
            .as_ref()
            .map(NativeTableConfig::project)
            .transpose()
    }

    /// Binds an already-registered table by name, describing it first.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// not-found, authorization, and transport failures are returned in
    /// [`NativeLifecycleResult`].
    // justification: napi boundary; a JavaScript string is primitive and cannot
    // be passed by reference, so the generated binding requires an owned String
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub async fn use_table_by_name(&self, table: String) -> NapiResult<NativeLifecycleResult> {
        match self.client.use_table_by_name(&table).await {
            Ok(()) => NativeLifecycleResult::success(&serde_json::Value::Null),
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// The active write binding, if any.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the binding cannot be projected.
    #[napi(getter)]
    pub fn table(&self) -> NapiResult<Option<NativeTableConfig>> {
        self.client
            .table()
            .as_ref()
            .map(NativeTableConfig::project)
            .transpose()
    }

    /// Enqueues one JSON row into the active table.
    ///
    /// Stays synchronous because the producer push is a bounded, non-blocking
    /// queue operation; only the drains need a worker thread. A saturated queue
    /// refuses here rather than dropping silently.
    ///
    /// # Errors
    ///
    /// Returns a napi error for an invalid card reference; no-active-table and
    /// queue-full refusals are returned in [`NativeLifecycleResult`].
    // justification: napi boundary; a JavaScript string is primitive and cannot
    // be passed by reference, so the generated binding requires an owned String
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub fn insert(
        &self,
        row: String,
        card_ref: Option<String>,
        run_id: Option<String>,
    ) -> NapiResult<NativeLifecycleResult> {
        let correlation = correlation(card_ref.as_deref(), run_id)?;
        match self.client.insert(row.into_bytes(), correlation) {
            Ok(()) => NativeLifecycleResult::success(&serde_json::Value::Null),
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Writes one already-built Arrow batch to `table` and awaits durability.
    ///
    /// The batch crosses as a single-batch Arrow IPC stream, the same framing
    /// [`NativeTableConfig::schema_ipc`] uses in the other direction, so the
    /// field metadata a canonical table declares survives the boundary. Unlike
    /// [`NativeBifrost::insert`] nothing is buffered, so no flush follows.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the bytes are not one single-batch Arrow IPC
    /// stream; server and envelope refusals are returned in
    /// [`NativeLifecycleResult`].
    // justification: napi boundary; the generated binding requires owned values
    #[allow(clippy::needless_pass_by_value)]
    #[napi]
    pub async fn write_batch(
        &self,
        table: String,
        batch_ipc: Buffer,
    ) -> NapiResult<NativeLifecycleResult> {
        let batch = decode_batch_ipc(&batch_ipc)?;
        match self.client.write_batch(&table, &batch).await {
            Ok(()) => NativeLifecycleResult::success(&serde_json::Value::Null),
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Flushes every pooled producer and awaits each durable acknowledgement.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// producer and sink failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn flush(&self) -> NapiResult<NativeLifecycleResult> {
        match self.client.flush().await {
            Ok(()) => NativeLifecycleResult::success(&serde_json::Value::Null),
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Drains every producer and stops its background task.
    ///
    /// # Errors
    ///
    /// As [`NativeBifrost::flush`].
    #[napi]
    pub async fn shutdown(&self) -> NapiResult<NativeLifecycleResult> {
        match self.client.shutdown().await {
            Ok(()) => NativeLifecycleResult::success(&serde_json::Value::Null),
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Number of distinct table producers currently pooled.
    #[napi(getter)]
    pub fn producer_count(&self) -> u32 {
        u32::try_from(self.client.producer_count()).unwrap_or(u32::MAX)
    }

    /// Starts one terminal-safe query through the Rust SDK owner.
    ///
    /// Expected request, Gate, authentication, and transport failures are
    /// returned in [`NativeQueryStart`] so JavaScript never has to parse an
    /// exception message to recover the public Wyrd error contract.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when napi cannot project the structured
    /// startup result itself.
    #[napi]
    pub async fn query(&self, request: NativeQueryRequest) -> NapiResult<NativeQueryStart> {
        let request = BifrostQueryRequest {
            sql: request.sql,
            visibility: match parse_visibility(&request.visibility) {
                Ok(visibility) => visibility,
                Err(error) => return Ok(NativeQueryStart::failure(&error)),
            },
            freshness: match parse_freshness(&request.freshness) {
                Ok(freshness) => freshness,
                Err(error) => return Ok(NativeQueryStart::failure(&error)),
            },
            deadline_ms: match request.deadline_ms.map(parse_deadline_ms).transpose() {
                Ok(deadline_ms) => deadline_ms,
                Err(error) => return Ok(NativeQueryStart::failure(&error)),
            },
        };
        Ok(match self.client.query(&request).await {
            Ok(stream) => NativeQueryStart::success(stream),
            Err(error) => NativeQueryStart::failure(&error),
        })
    }

    /// Lists active queries for the authenticated tenant.
    ///
    /// Cancelling the JavaScript promise abandons the pending HTTP request and
    /// does not create client-owned lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// Wyrd control failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn running(&self) -> NapiResult<NativeLifecycleResult> {
        match self.client.running().await {
            Ok(queries) => {
                NativeLifecycleResult::success(&serde_json::to_value(queries).map_err(napi_error)?)
            }
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Gets one active query by canonical request ID.
    ///
    /// Cancelling the JavaScript promise abandons the pending HTTP request and
    /// does not alter the active query.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// validation and Wyrd control failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn status(&self, request_id: String) -> NapiResult<NativeLifecycleResult> {
        let request_id = match parse_request_id(&request_id) {
            Ok(request_id) => request_id,
            Err(error) => {
                return Ok(NativeLifecycleResult::failure(&error));
            }
        };
        match self.client.status(&request_id).await {
            Ok(summary) => {
                NativeLifecycleResult::success(&serde_json::to_value(summary).map_err(napi_error)?)
            }
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Requests server-side cancellation without closing a local stream.
    ///
    /// Once the server accepts cancellation, abandoning the JavaScript promise
    /// does not reverse the server-side lifecycle transition.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// validation and Wyrd control failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn cancel(&self, request_id: String) -> NapiResult<NativeLifecycleResult> {
        let request_id = match parse_request_id(&request_id) {
            Ok(request_id) => request_id,
            Err(error) => {
                return Ok(NativeLifecycleResult::failure(&error));
            }
        };
        match self.client.cancel(&request_id).await {
            Ok(response) => {
                NativeLifecycleResult::success(&serde_json::to_value(response).map_err(napi_error)?)
            }
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }

    /// Describes one registered table's stored physical schema.
    ///
    /// The description is the server's own projection: the user fields a caller
    /// declares, the correlation inputs the write path resolves, the managed
    /// candidates it may supply, and a canonical table's physical fingerprint.
    /// JavaScript builds its insertable schema from this rather than from a
    /// local copy of the table contract.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// Wyrd control failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub async fn describe_table(
        &self,
        namespace: String,
        name: String,
    ) -> NapiResult<NativeLifecycleResult> {
        match self.client.describe(&format!("{namespace}.{name}")).await {
            Ok(description) => NativeLifecycleResult::success(
                &serde_json::to_value(description).map_err(napi_error)?,
            ),
            Err(error) => Ok(NativeLifecycleResult::failure(&error)),
        }
    }
}

impl NativeBifrostQueryStream {
    /// Wraps one Rust-owned query stream for napi iteration.
    fn new(stream: QueryResultStream) -> Self {
        let request_id = stream.request_id().as_str().to_owned();
        Self {
            request_id,
            stream: Arc::new(AsyncMutex::new(Some(Box::new(stream)))),
            terminal_json: Arc::new(Mutex::new(None)),
            schema_ipc: Arc::new(Mutex::new(None)),
        }
    }
}

/// Native query stream that retains Rust terminal validation and emits raw IPC.
#[napi]
pub struct NativeBifrostQueryStream {
    /// Canonical lifecycle request identity available before body polling.
    request_id: String,
    /// Mutable Rust query stream serialized across JavaScript `next` calls.
    stream: Arc<AsyncMutex<Option<Box<QueryResultStream>>>>,
    /// Validated serialized terminal retained after the Rust stream is released.
    terminal_json: Arc<Mutex<Option<String>>>,
    /// Schema-only Arrow IPC stream retained after the Rust stream is released.
    schema_ipc: Arc<Mutex<Option<Vec<u8>>>>,
}

#[napi]
impl NativeBifrostQueryStream {
    /// Returns the canonical server lifecycle request identity.
    #[napi(getter)]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Returns one Arrow IPC batch or the final validated terminal.
    ///
    /// # Errors
    ///
    /// Returns a napi error carrying the stable Wyrd code for failed,
    /// incomplete, malformed, or transport-terminated streams.
    #[napi]
    pub async fn next(&self) -> NapiResult<NativeQueryStep> {
        let mut stream_slot = self.stream.lock().await;
        let stream = stream_slot
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("query stream is closed".to_owned()))?;
        match stream.next_batch().await {
            Ok(Some(batch)) => match encode_batch(&batch) {
                Ok(ipc) => Ok(NativeQueryStep {
                    ipc: Some(Buffer::from(ipc)),
                    terminal_json: None,
                    error_code: None,
                    error_status: None,
                    error_title: None,
                    error_detail: None,
                    error_remediation: None,
                    error_details_json: None,
                }),
                Err(error) => {
                    *stream_slot = None;
                    Err(error)
                }
            },
            Ok(None) => {
                let schema = match stream.schema().map(encode_schema).transpose() {
                    Ok(schema) => schema,
                    Err(error) => {
                        *stream_slot = None;
                        return Err(error);
                    }
                };
                let terminal = if let Some(terminal) = stream.terminal() {
                    match serde_json::to_string(terminal) {
                        Ok(terminal) => terminal,
                        Err(error) => {
                            *stream_slot = None;
                            return Err(napi_error(error));
                        }
                    }
                } else {
                    *stream_slot = None;
                    return Err(sdk_error(&BifrostClientError::IncompleteQueryStream));
                };
                *stream_slot = None;
                *self
                    .terminal_json
                    .lock()
                    .map_err(|_| napi::Error::from_reason("terminal lock poisoned".to_owned()))? =
                    Some(terminal.clone());
                *self
                    .schema_ipc
                    .lock()
                    .map_err(|_| napi::Error::from_reason("schema lock poisoned".to_owned()))? =
                    schema;
                Ok(NativeQueryStep {
                    ipc: None,
                    terminal_json: Some(terminal),
                    error_code: None,
                    error_status: None,
                    error_title: None,
                    error_detail: None,
                    error_remediation: None,
                    error_details_json: None,
                })
            }
            Err(error) => {
                let terminal = stream
                    .terminal()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(napi_error);
                *stream_slot = None;
                let terminal = terminal?;
                if let Some(terminal) = terminal {
                    *self.terminal_json.lock().map_err(|_| {
                        napi::Error::from_reason("terminal lock poisoned".to_owned())
                    })? = Some(terminal);
                }
                let projected = wyrd_spec::error::WyrdError::from(&error);
                let problem = projected.as_problem_json();
                Ok(NativeQueryStep {
                    ipc: None,
                    terminal_json: None,
                    error_code: Some(projected.code().to_owned()),
                    error_status: Some(u32::from(projected.status())),
                    error_title: Some(problem_field(&problem, "title", projected.title())),
                    error_detail: Some(problem_field(&problem, "detail", &projected.to_string())),
                    error_remediation: Some(projected.remediation().to_owned()),
                    error_details_json: problem
                        .get("details")
                        .and_then(|value| serde_json::to_string(value).ok()),
                })
            }
        }
    }

    /// Drops the response stream so Rust transport cancellation propagates.
    #[napi]
    pub async fn close(&self) {
        *self.stream.lock().await = None;
    }

    /// Returns the result's authoritative schema as a schema-only IPC stream.
    ///
    /// Retained when the stream completes, so a query that produced no batch
    /// still carries the server's schema and JavaScript never has to infer one
    /// from the batches it happened to receive.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the retaining lock is poisoned.
    #[napi(getter)]
    pub fn schema_ipc(&self) -> NapiResult<Option<Buffer>> {
        self.schema_ipc
            .lock()
            .map(|schema| schema.clone().map(Buffer::from))
            .map_err(|_| napi::Error::from_reason("schema lock poisoned".to_owned()))
    }

    /// Returns serialized terminal metadata after validated completion.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the terminal lock is poisoned.
    #[napi(getter)]
    pub fn terminal_json(&self) -> NapiResult<Option<String>> {
        self.terminal_json
            .lock()
            .map(|terminal| terminal.clone())
            .map_err(|_| napi::Error::from_reason("terminal lock poisoned".to_owned()))
    }
}

/// Parses the optional per-row correlation at the Node boundary.
///
/// # Errors
///
/// Returns a napi error when `card_ref` is not one parsable Card reference.
fn correlation(card_ref: Option<&str>, run_id: Option<String>) -> NapiResult<Correlation> {
    Ok(Correlation {
        card_ref: card_ref
            .map(str::parse)
            .transpose()
            .map_err(|error| napi::Error::from_reason(format!("invalid cardRef: {error}")))?,
        run_id: run_id.map(RunId::from_string),
    })
}

/// Parses the native visibility spelling.
///
/// # Errors
///
/// Returns a structured SDK protocol error for an unknown value.
fn parse_visibility(value: &str) -> Result<VisibilityMode, BifrostClientError> {
    match value {
        "published_only" => Ok(VisibilityMode::PublishedOnly),
        "fused" => Ok(VisibilityMode::Fused),
        _ => Err(BifrostClientError::Protocol(
            "visibility must be published_only or fused".to_owned(),
        )),
    }
}

/// Converts a JavaScript deadline number into the shared signed request field.
///
/// Only exact integers pass through the lossless decimal round trip; the
/// `1..=u32::MAX` range is left to `BifrostQueryRequest::validate` so every
/// surface shares one check and one catalog error.
///
/// # Errors
///
/// Returns the shared query-contract validation error for a non-finite,
/// fractional, or beyond-`i64` number.
fn parse_deadline_ms(value: f64) -> Result<i64, BifrostClientError> {
    value.to_string().parse().map_err(|_| {
        BifrostClientError::Transport(WyrdError::Vala {
            error: BifrostError::QueryInvalidSql {
                detail: "deadline_ms must be an integer between 1 and 4294967295".to_owned(),
            },
        })
    })
}

/// Parses the native freshness spelling.
///
/// # Errors
///
/// Returns a structured SDK protocol error for an unknown value.
fn parse_freshness(value: &str) -> Result<FreshnessPolicy, BifrostClientError> {
    match value {
        "strict" => Ok(FreshnessPolicy::Strict),
        "allow_degraded" => Ok(FreshnessPolicy::AllowDegraded),
        _ => Err(BifrostClientError::Protocol(
            "freshness must be strict or allow_degraded".to_owned(),
        )),
    }
}

/// Parses one canonical request ID at the Node boundary.
///
/// # Errors
///
/// Returns the stable public validation error when the value is not a `UUIDv7` request ID.
fn parse_request_id(value: &str) -> Result<RequestId, BifrostClientError> {
    RequestId::parse(value).map_err(|error| {
        BifrostClientError::Transport(WyrdError::Validation {
            message: error.to_string(),
            details: serde_json::json!({"field": "request_id"}),
        })
    })
}

/// Encodes one decoded Rust record batch for TypeScript Apache Arrow.
///
/// # Errors
///
/// Returns a napi error when Arrow IPC encoding fails.
fn encode_batch(batch: &RecordBatch) -> NapiResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, batch.schema().as_ref())
        .map_err(napi_error)?;
    writer
        .write(batch)
        .and_then(|()| writer.finish())
        .map_err(napi_error)?;
    Ok(bytes)
}

/// Encodes one Arrow schema as a schema-only IPC stream with no batches.
///
/// The same encoding [`encode_batch`] uses, minus the batch, so the JavaScript
/// facade decodes an empty result exactly as it decodes a populated one.
///
/// # Errors
///
/// Returns a napi error when IPC writing fails.
fn encode_schema(schema: &SchemaRef) -> NapiResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, schema.as_ref())
        .map_err(napi_error)?;
    writer.finish().map_err(napi_error)?;
    Ok(bytes)
}

/// Reads one string member of an RFC 9457 problem payload.
///
/// The catalog owns the public text, so the projection reads it from the
/// problem document and falls back to the derive-backed field only if the
/// payload omits the member.
fn problem_field(problem: &Value, key: &str, fallback: &str) -> String {
    problem
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
        .to_owned()
}

/// Projects an SDK error with its stable code intact.
fn sdk_error(error: &BifrostClientError) -> Error {
    napi::Error::from_reason(format!(
        "[{}] {error}",
        wyrd_spec::error::WyrdError::from(error).code()
    ))
}

/// Converts an arbitrary boundary error into a napi failure.
fn napi_error(error: impl Display) -> Error {
    napi::Error::from_reason(error.to_string())
}
