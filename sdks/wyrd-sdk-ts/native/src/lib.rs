//! Thin napi projection of the Rust-owned `wyrd_client` capabilities:
//! the shared client, Bifrost, Cards, Verification, Operator connections,
//! Gateway administration, offline `WyrdState`, and the in-process `wyrd`
//! CLI commands.

#![deny(missing_docs)]

pub mod cards;
pub mod cli;
pub mod client;
pub mod gateway;
pub mod operators;
/// The napi projection of the tenant principal handle.
pub mod principals;
pub mod trace;
pub mod verification;

use std::collections::HashMap;
use std::result::Result as StdResult;
use std::sync::{Arc, Mutex};

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use napi::Env;
use napi::Error;
use napi::Result;
use napi::bindgen_prelude::PromiseRaw;
use napi::bindgen_prelude::{Buffer, Uint8Array};
use napi_derive::napi;
use serde::Serialize;
use serde_json::Value;
use std::fmt::Display;
use tokio::sync::{Mutex as AsyncMutex, Notify};
use wyrd_client::bifrost::Bifrost;
use wyrd_client::bifrost::Correlation;
use wyrd_client::bifrost::TableConfig;
use wyrd_client::bifrost::{BifrostClientError, QueryResultStream};
use wyrd_queue::QueueConfig;
use wyrd_queue::json_result::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_ROWS, JsonResultBuilder, admit_ceilings,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AlterTableRequest, BifrostQueryRequest, QueryParam};
use wyrd_spec::vala::api::{CompactionTypeWire, PhysicalLayoutWire};
use wyrd_spec::vala::error::BifrostError;
use wyrd_spec::vala::ids::{RunId, TraceId};
use wyrd_types::TimestampKind;

/// JavaScript query request projected onto the pure Wyrd contract.
#[napi(object)]
pub struct NativeQueryRequest {
    /// SELECT-only SQL text.
    pub sql: String,
    /// Optional query deadline in milliseconds, valid in `1..=u32::MAX`.
    ///
    /// Accepted as a JavaScript number so every out-of-range, fractional, or
    /// non-finite value reaches the structured startup failure instead of a
    /// napi binding error.
    pub deadline_ms: Option<f64>,
    /// Ordered bind values for the `$1..$n` placeholders in `sql`.
    pub params: Option<Vec<NativeQueryParam>>,
}

/// One JavaScript bind value, tagged by its JavaScript runtime type.
///
/// The TypeScript facade sets `kind` to `"null"`, `"boolean"`, `"number"`, or
/// `"string"` and fills the matching field; any other kind is refused by
/// [`NativeQueryParam::into_param`] so the catalog error comes from Rust.
#[napi(object)]
pub struct NativeQueryParam {
    /// JavaScript runtime type of the value.
    pub kind: String,
    /// Value when `kind` is `"boolean"`.
    pub bool: Option<bool>,
    /// Value when `kind` is `"number"`.
    pub number: Option<f64>,
    /// Value when `kind` is `"string"`.
    pub string: Option<String>,
}

impl NativeQueryParam {
    /// Largest integer a JavaScript number represents exactly (`2^53 - 1`).
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

    /// Projects this JavaScript value onto the shared typed bind value.
    ///
    /// A number that is an exact safe integer binds as an integer; any other
    /// number binds as a float, so a non-finite value reaches
    /// `BifrostQueryRequest::validate` and its one catalog error.
    ///
    /// # Errors
    ///
    /// Returns the shared invalid-query error when `kind` is unsupported or
    /// its matching field is absent.
    fn into_param(self, index: usize) -> StdResult<QueryParam, BifrostClientError> {
        let unsupported = || {
            BifrostClientError::Transport(WyrdError::Vala {
                error: BifrostError::QueryInvalidSql {
                    detail: format!(
                        "params[{index}] must be null, a boolean, a number, or a string"
                    ),
                },
            })
        };
        match (self.kind.as_str(), self.bool, self.number, self.string) {
            ("null", ..) => Ok(QueryParam::Null),
            ("boolean", Some(value), ..) => Ok(QueryParam::Bool(value)),
            ("number", _, Some(value), _) => Ok(
                // The decimal round trip is exact for a safe integer.
                match value.to_string().parse() {
                    Ok(integer) if value.abs() <= Self::MAX_SAFE_INTEGER => {
                        QueryParam::Int(integer)
                    }
                    _ => QueryParam::Float(value),
                },
            ),
            ("string", _, _, Some(value)) => Ok(QueryParam::String(value)),
            _ => Err(unsupported()),
        }
    }
}

impl NativeQueryStep {
    /// Projects one stable Wyrd error onto a failed step's structured fields.
    fn failure(projected: &WyrdError) -> Self {
        let problem = projected.as_problem_json();
        Self {
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
        }
    }
}

/// One raw native iterator step consumed by the TypeScript Arrow facade.
#[napi(object)]
pub struct NativeQueryStep {
    /// Complete Arrow IPC stream containing exactly one record batch.
    pub ipc: Option<Buffer>,
    /// Serialized validated terminal, present only on the final step; from
    /// `collect`, the whole bounded `{columns, rows, terminal}` result.
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
    fn success(value: &Value) -> Result<Self> {
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
    fn outcome<T: Serialize>(result: StdResult<T, WyrdError>) -> Result<Self> {
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

impl NativeBifrostConnection {
    /// Project one connection outcome onto the closed result, so every
    /// constructor path reports a failure through the same catalog shape.
    fn from_outcome(connected: StdResult<Bifrost, BifrostClientError>) -> Self {
        match connected {
            Ok(handle) => Self {
                bifrost: Some(NativeBifrost {
                    client: Arc::new(handle),
                }),
                error: None,
            },
            Err(error) => Self {
                bifrost: None,
                error: Some(NativeWyrdError::from_wyrd(&WyrdError::from(&error))),
            },
        }
    }
}

/// Closed result of declaring or describing one table: its config or a
/// catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeTableConfigResult {
    /// The table config when it was declared or described.
    pub config: Option<NativeTableConfig>,
    /// Catalog failure when declaration or description failed.
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
    fn project(config: &TableConfig) -> Result<Self> {
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
    fn parse(&self) -> Result<TableConfig> {
        serde_json::from_str(&self.config_json).map_err(napi_error)
    }
}

/// Decodes one single-batch Arrow IPC stream into a native record batch.
///
/// # Errors
///
/// Returns a napi error when the bytes are not one Arrow IPC stream carrying
/// exactly one batch, which is what one logical write is.
fn decode_batch_ipc(bytes: &[u8]) -> Result<RecordBatch> {
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
/// A catalogued refusal of the declaration itself — a table name that is not
/// `namespace.name`, a document that does not map to columns, an open-extras
/// object, or a server-owned column — returns in the result's structured
/// `error`, which the TypeScript wrapper raises as a `WyrdError` carrying its
/// code, status, details, and remediation.
///
/// # Errors
///
/// Returns a napi error only when the declared config cannot be encoded. A
/// table that is not `namespace.name`, a document that is not one mappable
/// JSON Schema, a server-owned column, a layout that is not one
/// physical-layout declaration, a compaction target that is not a
/// non-negative integer, or a compaction type that is not one known
/// hyphenated wire spelling is returned as catalog metadata.
#[napi]
pub fn table_config_from_json_schema(
    table: String,
    schema_json: String,
    layout_json: Option<String>,
    compaction_target_file_size_bytes: Option<f64>,
    compaction_type: Option<String>,
) -> Result<NativeTableConfigResult> {
    let declared = serde_json::from_str::<Value>(&schema_json)
        .map_err(|error| validation_error(format!("invalid JSON schema: {error}"), "schema"))
        .and_then(|schema| TableConfig::from_json_schema(&table, &schema))
        .and_then(|config| {
            declare(
                config,
                layout_json.as_deref(),
                compaction_target_file_size_bytes,
                compaction_type.as_deref(),
            )
        });
    NativeTableConfigResult::declared(declared)
}

/// Builds one table config from an explicit Arrow schema.
///
/// This is the door for column types JSON Schema cannot express. The schema
/// crosses as one schema-only Arrow IPC stream, the framing
/// [`NativeTableConfig::schema_ipc`] uses in the other direction; layout and
/// compaction apply as in [`table_config_from_json_schema`].
///
/// # Errors
///
/// Returns a napi error only when the declared config cannot be encoded. Bytes
/// that are not one Arrow IPC stream and every refusal
/// [`table_config_from_json_schema`] lists are returned as catalog metadata.
#[napi]
pub fn table_config_from_arrow_ipc(
    table: String,
    schema_ipc: Buffer,
    layout_json: Option<String>,
    compaction_target_file_size_bytes: Option<f64>,
    compaction_type: Option<String>,
) -> Result<NativeTableConfigResult> {
    let declared =
        arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(schema_ipc.as_ref()), None)
            .map_err(|error| validation_error(format!("invalid Arrow schema: {error}"), "schema"))
            .and_then(|reader| TableConfig::from_arrow(&table, reader.schema()))
            .and_then(|config| {
                declare(
                    config,
                    layout_json.as_deref(),
                    compaction_target_file_size_bytes,
                    compaction_type.as_deref(),
                )
            });
    NativeTableConfigResult::declared(declared)
}

/// Decodes one Variant cell's `metadata`/`value` bytes into its native
/// JavaScript value.
///
/// The typed row terminal calls this for every Variant cell it finds while
/// walking the result schema, so decoding stays in the shared `wyrd-queue`
/// owner. napi's JSON projection makes objects plain objects, arrays arrays,
/// and an integer outside the IEEE-754 safe range a `bigint`.
///
/// # Errors
///
/// Returns a napi error when the bytes are not a valid Variant, which only a
/// faulty server result can produce.
#[napi]
pub fn variant_to_value(metadata: Uint8Array, value: Uint8Array) -> Result<Value> {
    wyrd_queue::variant::variant_bytes_to_json(&metadata, &value).map_err(|violation| {
        napi_error(format!(
            "query result Variant does not decode: {violation:?}"
        ))
    })
}

/// The JSON Schema `format` a Bifrost timestamp type declares itself with.
///
/// `kind` is a [`TimestampKind::name`]; returns `None` for an unknown type. The
/// TypeScript Wyrd timestamp types read their format here rather than restating
/// the format table.
#[napi]
pub fn timestamp_format(kind: String) -> Option<&'static str> {
    TimestampKind::from_name(&kind).map(TimestampKind::json_format)
}

/// Checks one timestamp text against a Bifrost timestamp type and returns its
/// canonical text.
///
/// `kind` is a [`TimestampKind::name`]. The text is parsed exactly as the write
/// path parses it, so a value the TypeScript Wyrd types accept is one an
/// insert accepts. Returns `None` for an unknown type or refused text.
#[napi]
pub fn timestamp_text(kind: String, text: String) -> Option<String> {
    TimestampKind::from_name(&kind)?.canonical(&text)
}

/// Renders one stored timestamp cell as its Bifrost timestamp type's text.
///
/// `kind` is a [`TimestampKind::name`] and `stored` the column's microseconds;
/// a `TIMESTAMP_TZ` cell also passes its `local` writer reading. The typed row
/// terminal calls this for every timestamp cell, so reads render in Rust.
/// Returns `None` for an unknown type or a value out of range.
#[napi]
pub fn stored_timestamp_text(kind: String, stored: i64, local: Option<i64>) -> Option<String> {
    TimestampKind::from_name(&kind)?.render(stored, local)
}

/// The Bifrost timestamp type of every top-level timestamp column in a
/// schema-only Arrow IPC stream.
///
/// Maps each column to `TIMESTAMP_NTZ`, `TIMESTAMP_LTZ`, or `TIMESTAMP_TZ` as
/// [`TimestampKind::of`] decides it, so the typed row terminal renders each
/// column's cells without restating that decision.
///
/// # Errors
///
/// Returns a napi error when the bytes are not an Arrow IPC stream.
#[napi]
pub fn timestamp_kinds(schema_ipc: Buffer) -> Result<HashMap<String, &'static str>> {
    let schema = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(schema_ipc), None)
        .map_err(napi_error)?
        .schema();
    Ok(schema
        .fields()
        .iter()
        .filter_map(|field| {
            TimestampKind::of(field.data_type()).map(|kind| (field.name().clone(), kind.name()))
        })
        .collect())
}

impl NativeTableConfigResult {
    /// Projects one declaration outcome, its refusal as catalog metadata.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the declared config cannot be encoded.
    fn declared(declared: StdResult<TableConfig, BifrostClientError>) -> Result<Self> {
        match declared {
            Ok(config) => Ok(Self {
                config: Some(NativeTableConfig::project(&config)?),
                error: None,
            }),
            Err(error) => Ok(Self {
                config: None,
                error: Some(NativeWyrdError::from_wyrd(&WyrdError::from(&error))),
            }),
        }
    }
}

/// Applies a declaration's serialized layout and compaction options to its
/// base config, whichever schema door built it.
///
/// # Errors
///
/// Returns the shared validation refusal for unparsable layout text, a
/// non-integer compaction target, and an unknown compaction type.
fn declare(
    config: TableConfig,
    layout_json: Option<&str>,
    compaction_target_file_size_bytes: Option<f64>,
    compaction_type: Option<&str>,
) -> StdResult<TableConfig, BifrostClientError> {
    let config = apply_layout(config, layout_json)?;
    apply_compaction_type(
        apply_compaction_target(config, compaction_target_file_size_bytes)?,
        compaction_type,
    )
}

/// Applies one optional serialized physical layout to a config.
///
/// # Errors
///
/// Returns the shared validation refusal when the text is not one
/// `PhysicalLayoutWire`.
fn apply_layout(
    config: TableConfig,
    layout_json: Option<&str>,
) -> StdResult<TableConfig, BifrostClientError> {
    match layout_json {
        None => Ok(config),
        Some(layout) => {
            let layout: PhysicalLayoutWire = serde_json::from_str(layout).map_err(|error| {
                validation_error(format!("invalid physical layout: {error}"), "layout")
            })?;
            Ok(config.with_layout(layout))
        }
    }
}

/// Applies one optional explicit Forge compaction file target to a config.
///
/// JavaScript numbers arrive as `f64`; only an exact non-negative integer
/// survives the lossless decimal round trip into `u64`. The accepted byte
/// range stays the server's registration check, so every SDK shares one
/// catalog error for an out-of-range target.
///
/// # Errors
///
/// Returns the shared validation refusal for a non-finite, fractional, or
/// negative number.
fn apply_compaction_target(
    config: TableConfig,
    bytes: Option<f64>,
) -> StdResult<TableConfig, BifrostClientError> {
    match bytes {
        None => Ok(config),
        Some(bytes) => {
            let bytes: u64 = bytes.to_string().parse().map_err(|_| {
                validation_error(
                    format!(
                        "compaction target must be a non-negative integer byte count, got {bytes}"
                    ),
                    "compactionTargetFileSizeBytes",
                )
            })?;
            Ok(config.with_compaction_target_file_size_bytes(bytes))
        }
    }
}

/// Applies one optional explicit Forge compaction type to a config.
///
/// The spelling is the hyphenated wire value, parsed by the wire type's own
/// serde contract so JavaScript accepts exactly what the server accepts.
///
/// # Errors
///
/// Returns the shared validation refusal when the spelling is not one known
/// compaction type.
fn apply_compaction_type(
    config: TableConfig,
    raw: Option<&str>,
) -> StdResult<TableConfig, BifrostClientError> {
    match raw {
        None => Ok(config),
        Some(raw) => {
            let kind: CompactionTypeWire = serde_json::from_value(Value::String(raw.to_owned()))
                .map_err(|error| {
                    validation_error(
                        format!("invalid compaction type: {error}"),
                        "compactionType",
                    )
                })?;
            Ok(config.with_compaction_type(kind))
        }
    }
}

/// The queue configuration for an optional JavaScript byte-budget override.
///
/// `None` keeps the 256 MiB default. A negative number cannot be a budget, so
/// it becomes zero, which connecting refuses with
/// `WYRD_CLIENT_400_CONFIG_INVALID` like any budget too small to seal one
/// message.
pub(crate) fn queue_config(client_byte_limit_bytes: Option<i64>) -> QueueConfig {
    QueueConfig::with_client_byte_limit(
        client_byte_limit_bytes.map(|limit| usize::try_from(limit).unwrap_or(0)),
    )
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
    pub fn register<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            match client.register().await {
                Ok(outcome) => NativeLifecycleResult::success(&serde_json::Value::String(
                    wyrd_client::bifrost::register_outcome_name(outcome).to_owned(),
                )),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
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
    #[napi]
    pub fn use_table(&self, table: NativeTableConfig) -> Result<Option<NativeTableConfig>> {
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
    #[napi]
    pub fn use_table_by_name<'env>(
        &self,
        env: &'env Env,
        table: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            match client.use_table_by_name(&table).await {
                Ok(()) => NativeLifecycleResult::success(&serde_json::Value::Null),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
    }

    /// The active write binding, if any.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the binding cannot be projected.
    #[napi(getter)]
    pub fn table(&self) -> Result<Option<NativeTableConfig>> {
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
    /// Returns a napi error only when the outcome cannot be encoded; an invalid
    /// Card UID, no-active-table, and queue-full refusals are returned in
    /// [`NativeLifecycleResult`].
    #[napi]
    pub fn insert(
        &self,
        env: Env,
        row: String,
        subject_card_uid: Option<String>,
        run_id: Option<String>,
    ) -> Result<NativeLifecycleResult> {
        let inserted = correlation(subject_card_uid.as_deref(), run_id).and_then(|correlation| {
            crate::trace::traced(env, || self.client.insert(row.into_bytes(), correlation))
        });
        match inserted {
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
    #[napi]
    pub fn write_batch<'env>(
        &self,
        env: &'env Env,
        table: String,
        batch_ipc: Buffer,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            let batch = decode_batch_ipc(&batch_ipc)?;
            match client.write_batch(&table, &batch).await {
                Ok(()) => NativeLifecycleResult::success(&serde_json::Value::Null),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
    }

    /// Flushes every pooled producer and awaits each durable acknowledgement.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// producer and sink failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub fn flush<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            match client.flush().await {
                Ok(()) => NativeLifecycleResult::success(&serde_json::Value::Null),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
    }

    /// Drains every producer and stops its background task.
    ///
    /// # Errors
    ///
    /// As [`NativeBifrost::flush`].
    #[napi]
    pub fn shutdown<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            match client.shutdown().await {
                Ok(()) => NativeLifecycleResult::success(&serde_json::Value::Null),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
    }

    /// Number of distinct table producers currently pooled.
    #[napi(getter)]
    pub fn producer_count(&self) -> u32 {
        u32::try_from(self.client.producer_count()).unwrap_or(u32::MAX)
    }

    /// Rows dropped by the fire-and-forget observation path, saturating at
    /// `i64::MAX`.
    ///
    /// Always zero for [`NativeBifrost::insert`], which refuses rather than
    /// drops.
    #[napi(getter)]
    pub fn dropped(&self) -> i64 {
        i64::try_from(self.client.dropped()).unwrap_or(i64::MAX)
    }

    /// Records one telemetry row fire-and-forget through
    /// [`wyrd_client::bifrost::observe::record`].
    ///
    /// The row names its own `table` and JSON Schema rather than this client's
    /// bound table, so instrumentation never disturbs the application's
    /// binding. A producer refusal, such as a full queue, is counted on
    /// [`NativeBifrost::dropped`] instead of returned. The first row for a
    /// table this client has not described records on a background task
    /// once the describe returns its table version; a failed describe is
    /// counted the same way. That record is admitted before the task starts,
    /// so a following `flush` or `shutdown` waits for it.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be encoded or the
    /// background describe cannot be scheduled; an
    /// unparsable or unmappable schema and an invalid Card UID are
    /// returned in [`NativeLifecycleResult`].
    #[napi]
    pub fn record(
        &self,
        env: Env,
        table: String,
        schema_json: String,
        row: String,
        subject_card_uid: Option<String>,
        run_id: Option<String>,
    ) -> Result<NativeLifecycleResult> {
        let (schema, correlation) = match telemetry_schema(&schema_json)
            .and_then(|schema| Ok((schema, correlation(subject_card_uid.as_deref(), run_id)?)))
        {
            Ok(parsed) => parsed,
            Err(error) => return Ok(NativeLifecycleResult::failure(&error)),
        };
        let row = row.into_bytes();
        if let Some(described) = self.client.cached_writer_table(&table) {
            crate::trace::traced(env, || {
                wyrd_client::bifrost::observe::record(
                    &self.client,
                    &described.with_schema(schema),
                    row,
                    correlation,
                );
            });
        } else {
            let client = Arc::clone(&self.client);
            let admitted = client.admit_record();
            crate::trace::spawn(&env, async move {
                wyrd_client::bifrost::observe::describe_and_record(
                    &client,
                    &table,
                    schema,
                    row,
                    correlation,
                    admitted,
                )
                .await;
                Ok(())
            })?;
        }
        NativeLifecycleResult::success(&serde_json::Value::Null)
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
    pub fn query<'env>(
        &self,
        env: &'env Env,
        request: NativeQueryRequest,
    ) -> napi::Result<PromiseRaw<'env, NativeQueryStart>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            let params = request
                .params
                .unwrap_or_default()
                .into_iter()
                .enumerate()
                .map(|(index, param)| param.into_param(index))
                .collect::<StdResult<Vec<_>, _>>();
            let params = match params {
                Ok(params) => params,
                Err(error) => return Ok(NativeQueryStart::failure(&error)),
            };
            let request = BifrostQueryRequest {
                params,
                sql: request.sql,
                deadline_ms: match request.deadline_ms.map(parse_deadline_ms).transpose() {
                    Ok(deadline_ms) => deadline_ms,
                    Err(error) => return Ok(NativeQueryStart::failure(&error)),
                },
            };
            Ok(match client.query(&request).await {
                Ok(stream) => NativeQueryStart::success(stream),
                Err(error) => NativeQueryStart::failure(&error),
            })
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
    pub fn running<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            match client.running().await {
                Ok(queries) => NativeLifecycleResult::success(
                    &serde_json::to_value(queries).map_err(napi_error)?,
                ),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
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
    pub fn status<'env>(
        &self,
        env: &'env Env,
        request_id: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            let request_id = match parse_request_id(&request_id) {
                Ok(request_id) => request_id,
                Err(error) => {
                    return Ok(NativeLifecycleResult::failure(&error));
                }
            };
            match client.status(&request_id).await {
                Ok(summary) => NativeLifecycleResult::success(
                    &serde_json::to_value(summary).map_err(napi_error)?,
                ),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
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
    pub fn cancel<'env>(
        &self,
        env: &'env Env,
        request_id: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            let request_id = match parse_request_id(&request_id) {
                Ok(request_id) => request_id,
                Err(error) => {
                    return Ok(NativeLifecycleResult::failure(&error));
                }
            };
            match client.cancel(&request_id).await {
                Ok(response) => NativeLifecycleResult::success(
                    &serde_json::to_value(response).map_err(napi_error)?,
                ),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
    }

    /// Reads one trace's span tree, plus each trace linking directly to it,
    /// as canonical span records.
    ///
    /// The id is parsed before any request, so a malformed one never reaches
    /// the server; the read itself is `Bifrost::trace_timeline`.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the spans cannot be projected; an
    /// invalid id and query or transport failures are returned in
    /// [`NativeLifecycleResult`].
    #[napi]
    pub fn trace_timeline<'env>(
        &self,
        env: &'env Env,
        trace_id: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            let spans = match TraceId::from_hex(&trace_id) {
                Ok(trace_id) => client
                    .trace_timeline(trace_id)
                    .await
                    .map_err(|error| WyrdError::from(&error)),
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(spans)
        })
    }

    /// Lists the compact entries of every table the caller may read.
    ///
    /// The server's authorized `BifrostTableEntry` values, unchanged and
    /// without schemas; JavaScript describes one table with `describeTable`
    /// only when it needs that schema.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// Wyrd control failures are returned in [`NativeLifecycleResult`].
    #[napi]
    pub fn list_tables<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            match client.list_tables().await {
                Ok(entries) => NativeLifecycleResult::success(
                    &serde_json::to_value(entries).map_err(napi_error)?,
                ),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
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
    pub fn describe_table<'env>(
        &self,
        env: &'env Env,
        namespace: String,
        name: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            match client.describe_table(&namespace, &name).await {
                Ok(description) => NativeLifecycleResult::success(
                    &serde_json::to_value(description).map_err(napi_error)?,
                ),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
    }

    /// Alters one registered table's definition at its observed table version
    /// and resolves to the committed description.
    ///
    /// `request_json` is one serialized `AlterTableRequest`, decoded strictly
    /// here; every edit decision belongs to the server.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the native result cannot be projected;
    /// a malformed request and every Wyrd control failure are returned in
    /// [`NativeLifecycleResult`].
    ///
    /// # Cancellation
    ///
    /// The edit runs to completion once started; a lost or abandoned result
    /// may leave it committed, so describe the table before retrying.
    #[napi]
    pub fn alter_table<'env>(
        &self,
        env: &'env Env,
        namespace: String,
        name: String,
        request_json: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            let altered = match serde_json::from_str::<AlterTableRequest>(&request_json) {
                Ok(request) => client.alter_table(&namespace, &name, &request).await,
                Err(error) => Err(validation_error(
                    format!("invalid table edit request: {error}"),
                    "request",
                )),
            };
            match altered {
                Ok(description) => NativeLifecycleResult::success(
                    &serde_json::to_value(description).map_err(napi_error)?,
                ),
                Err(error) => Ok(NativeLifecycleResult::failure(&error)),
            }
        })
    }
}

impl NativeBifrostQueryStream {
    /// Wraps one Rust-owned query stream for napi iteration.
    fn new(stream: QueryResultStream) -> Self {
        let request_id = stream.request_id().as_str().to_owned();
        Self {
            request_id,
            stream: Arc::new(AsyncMutex::new(Some(Box::new(stream)))),
            closing: Arc::new(Notify::new()),
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
    /// Signalled by [`Self::close`] before it waits for the stream lock, so a
    /// [`Self::collect`] holding that lock stops draining instead of blocking it.
    closing: Arc<Notify>,
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
    pub async fn next(&self) -> Result<NativeQueryStep> {
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
                Ok(NativeQueryStep::failure(&WyrdError::from(&error)))
            }
        }
    }

    /// Drains the whole stream into one bounded `{columns, rows, terminal}` JSON text.
    ///
    /// An omitted ceiling takes the bounded-result default, 1000 rows or 4 MiB;
    /// a supplied one must be an exact integer in the same admitted range the MCP tool enforces
    /// ([`admit_ceilings`]).
    /// The projection is [`JsonResultBuilder`], the same owner the server's MCP
    /// `bifrost.query` tool builds through, so both surfaces return the same
    /// value for the same result. The stream is released when this returns.
    ///
    /// # Errors
    ///
    /// Returns a step carrying the stable Wyrd error for a ceiling outside its
    /// admitted range, for a failed, incomplete, or malformed stream, and for a
    /// result past `max_rows` or `max_bytes`, and
    /// a napi error when the stream is already closed or [`Self::close`] stops
    /// the drain.
    #[napi]
    pub async fn collect(
        &self,
        max_rows: Option<f64>,
        max_bytes: Option<f64>,
    ) -> Result<NativeQueryStep> {
        let mut stream_slot = self.stream.lock().await;
        let mut stream = stream_slot
            .take()
            .ok_or_else(|| napi::Error::from_reason("query stream is closed".to_owned()))?;
        // The stream is already taken, so every refusal below drops and releases it.
        let (max_rows, max_bytes) = match (
            max_rows
                .map(|value| parse_ceiling(value, "max_rows"))
                .transpose(),
            max_bytes
                .map(|value| parse_ceiling(value, "max_bytes"))
                .transpose(),
        ) {
            (Ok(max_rows), Ok(max_bytes)) => (max_rows, max_bytes),
            (Err(error), _) | (_, Err(error)) => {
                return Ok(NativeQueryStep::failure(&WyrdError::from(&error)));
            }
        };
        let max_rows = max_rows.unwrap_or(DEFAULT_MAX_ROWS);
        let max_bytes = max_bytes
            .map_or(Ok(DEFAULT_MAX_BYTES), usize::try_from)
            .unwrap_or(usize::MAX);
        if let Err(refused) = admit_ceilings(max_rows, max_bytes) {
            return Ok(NativeQueryStep::failure(&WyrdError::Validation {
                message: refused.reason.to_owned(),
                details: refused.details,
            }));
        }
        let mut result =
            JsonResultBuilder::new(usize::try_from(max_rows).unwrap_or(usize::MAX), max_bytes);
        let mut columns = false;
        loop {
            let next = tokio::select! {
                biased;
                () = self.closing.notified() => {
                    return Err(napi::Error::from_reason("query stream is closed".to_owned()));
                }
                next = stream.next_batch() => next,
            };
            let batch = match next {
                Ok(batch) => batch,
                Err(error) => return Ok(NativeQueryStep::failure(&WyrdError::from(&error))),
            };
            let schema = if columns { None } else { stream.schema() };
            columns |= schema.is_some();
            let done = batch.is_none();
            let accepted = schema
                .map_or(Ok(()), |schema| result.accept_schema(schema))
                .and_then(|()| batch.map_or(Ok(()), |batch| result.accept_batch(&batch)));
            if let Err(error) = accepted {
                return Ok(NativeQueryStep::failure(&error));
            }
            if done {
                break;
            }
        }
        let Some(terminal) = stream.terminal() else {
            return Ok(NativeQueryStep::failure(&WyrdError::from(
                &BifrostClientError::IncompleteQueryStream,
            )));
        };
        match result.finish(terminal) {
            Ok(value) => Ok(NativeQueryStep {
                ipc: None,
                terminal_json: Some(value.to_string()),
                error_code: None,
                error_status: None,
                error_title: None,
                error_detail: None,
                error_remediation: None,
                error_details_json: None,
            }),
            Err(error) => Ok(NativeQueryStep::failure(&error)),
        }
    }

    /// Drops the response stream so Rust transport cancellation propagates.
    ///
    /// A running [`Self::collect`] is signalled first, so it releases the
    /// stream lock instead of draining the rest of the result.
    #[napi]
    pub async fn close(&self) {
        self.closing.notify_one();
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
    pub fn schema_ipc(&self) -> Result<Option<Buffer>> {
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
    pub fn terminal_json(&self) -> Result<Option<String>> {
        self.terminal_json
            .lock()
            .map(|terminal| terminal.clone())
            .map_err(|_| napi::Error::from_reason("terminal lock poisoned".to_owned()))
    }
}

/// Maps one telemetry row's JSON Schema text to its Arrow schema through the
/// shared `wyrd-queue` mapper.
///
/// # Errors
///
/// Returns the shared validation refusal for unparsable text and the mapper's
/// error for a document that does not map to Arrow.
fn telemetry_schema(schema_json: &str) -> StdResult<SchemaRef, BifrostClientError> {
    let schema: Value = serde_json::from_str(schema_json)
        .map_err(|error| validation_error(format!("invalid JSON schema: {error}"), "schema"))?;
    Ok(Arc::new(wyrd_types::json_schema_to_arrow(&schema)?))
}

/// Parses the optional per-row correlation at the Node boundary.
///
/// # Errors
///
/// Returns the shared validation refusal when `subject_card_uid` is not one parsable
/// Card UID.
fn correlation(
    subject_card_uid: Option<&str>,
    run_id: Option<String>,
) -> StdResult<Correlation, BifrostClientError> {
    Ok(Correlation {
        subject_card_uid: subject_card_uid
            .map(str::parse)
            .transpose()
            .map_err(|error| {
                validation_error(format!("invalid subjectCardUid: {error}"), "subjectCardUid")
            })?,
        run_id: run_id.map(RunId::from_string),
    })
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
fn parse_deadline_ms(value: f64) -> StdResult<i64, BifrostClientError> {
    value.to_string().parse().map_err(|_| {
        BifrostClientError::Transport(WyrdError::Vala {
            error: BifrostError::QueryInvalidSql {
                detail: "deadline_ms must be an integer between 1 and 4294967295".to_owned(),
            },
        })
    })
}

/// Converts one JavaScript collection ceiling into the integer [`admit_ceilings`] takes.
///
/// Only exact integers in `0..=u32::MAX` pass through the lossless decimal round
/// trip, so a fractional or wrapping number is refused instead of being narrowed
/// into an admitted value; the admitted range itself stays with [`admit_ceilings`].
///
/// # Errors
///
/// Returns the catalog validation error naming `field` for a non-finite,
/// fractional, negative, or beyond-`u32` number.
fn parse_ceiling(value: f64, field: &str) -> StdResult<u32, BifrostClientError> {
    value.to_string().parse().map_err(|_| {
        validation_error(
            format!("{field} must be an integer in its allowed range"),
            field,
        )
    })
}

/// Parses one canonical request ID at the Node boundary.
///
/// # Errors
///
/// Returns the stable public validation error when the value is not a `UUIDv7` request ID.
fn parse_request_id(value: &str) -> StdResult<RequestId, BifrostClientError> {
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
fn encode_batch(batch: &RecordBatch) -> Result<Vec<u8>> {
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
fn encode_schema(schema: &SchemaRef) -> Result<Vec<u8>> {
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

/// Builds the catalog validation refusal for one malformed JavaScript argument.
///
/// Local argument checks share `WYRD_SPEC_400_VALIDATION`, so a caller reads
/// the same stable code for a bad argument as the server returns for a bad
/// request field; `field` names the offending argument in `details`.
fn validation_error(message: String, field: &str) -> BifrostClientError {
    BifrostClientError::Transport(WyrdError::Validation {
        message,
        details: serde_json::json!({"field": field}),
    })
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
