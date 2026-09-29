//! The Eval authoring projection: caller context → canonical record → fixed row.
//!
//! `observe.eval(...)` owns this conversion, so the shared Bifrost facade and
//! `wyrd-queue` never deserialize Eval context or dispatch on a Verifier kind.

use arrow_schema::DataType;
use chrono::Utc;
use opentelemetry::trace::TraceContextExt;
use serde_json::{Value, json};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::eval::media::MediaRef;
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::ids::{RecordId, SessionId, SpanId, TraceId};

use crate::bifrost::WriterTable;
use crate::observe::{ProjectedColumn, invalid_observation, require_projection, row_bytes};

/// The optional per-emission data an Eval observation may carry.
///
/// SDK-only authoring sugar over the existing canonical record: it introduces no
/// second durable observation type, and Python keyword arguments and the
/// TypeScript options object expose exactly these fields.
#[derive(Debug, Default, Clone)]
pub struct EvalObservationOptions {
    /// The observed interaction's session, which is a fact about the
    /// interaction rather than about the run.
    pub session_id: Option<SessionId>,
    /// Object-storage descriptors for media the judge Prompt binds by `id`.
    pub media: Vec<MediaRef>,
    /// Explicit trace identity, which wins over the active span.
    pub trace_id: Option<TraceId>,
    /// Explicit span identity within `trace_id`.
    pub span_id: Option<SpanId>,
}

impl EvalObservationOptions {
    /// Build options from a foreign runtime's string and JSON parts.
    ///
    /// The Python keyword arguments and the TypeScript options object both
    /// arrive as text, so both boundaries parse here instead of each owning a
    /// copy of the identifier and media rules.
    ///
    /// # Errors
    /// Returns `WYRD_SPEC_400_VALIDATION` when `session_id` is not a UUID,
    /// `trace_id` or `span_id` is not its hex identifier, or `media_json` is not
    /// one JSON array of media descriptors.
    pub fn from_parts(
        session_id: Option<&str>,
        media_json: Option<&str>,
        trace_id: Option<&str>,
        span_id: Option<&str>,
    ) -> Result<Self, WyrdError> {
        Ok(Self {
            session_id: parse_session_id(session_id)?,
            media: match media_json {
                Some(media) => {
                    serde_json::from_str(media).map_err(|error| WyrdError::Validation {
                        message: format!("observe.eval media is invalid: {error}"),
                        details: json!({ "field": "media" }),
                    })?
                }
                None => Vec::new(),
            },
            trace_id: trace_id.map(TraceId::from_hex).transpose()?,
            span_id: span_id.map(SpanId::from_hex).transpose()?,
        })
    }
}

/// Parse an optional session identifier supplied by a foreign runtime.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` when the text is not a UUID.
pub fn parse_session_id(session_id: Option<&str>) -> Result<Option<SessionId>, WyrdError> {
    session_id
        .map(|text| {
            text.parse::<uuid::Uuid>()
                .map(SessionId)
                .map_err(|error| WyrdError::Validation {
                    message: format!("session_id is not a UUID: {error}"),
                    details: json!({ "field": "session_id" }),
                })
        })
        .transpose()
}

/// The user columns `eval(...)` projects, in `vala.eval.observations` order.
fn projection_columns() -> [ProjectedColumn; 7] {
    [
        ("record_id", DataType::Utf8, false),
        ("session_id", DataType::Utf8, true),
        ("context", DataType::Utf8, false),
        ("trace_id", DataType::FixedSizeBinary(16), true),
        ("span_id", DataType::FixedSizeBinary(8), true),
        (
            "created_at",
            DataType::Timestamp(arrow_schema::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        ("media", DataType::Utf8, true),
    ]
}

/// Verify a described fixed Eval table accepts this projection.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when the described authored
/// fields are not exactly this projection's ordered name, type, and
/// nullability sequence.
pub(crate) fn require_projection_columns(table: &WriterTable) -> Result<(), WyrdError> {
    require_projection(table, &projection_columns())
}

/// Build the canonical Eval observation from caller context and options.
///
/// The call mints `record_id` and `created_at`; `run_id` and the observed
/// subject travel as Bifrost correlation, not as record fields. Trace identity
/// is explicit-first: an explicit `trace_id` is used with only the explicit
/// `span_id`, because the active span's id belongs to the active trace and
/// pairing the two would name a span that never existed in that trace.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when a media descriptor is not
/// serializable, and the record's own validation error when `span_id` is
/// supplied without `trace_id`.
pub(crate) fn observation(
    context: Value,
    options: EvalObservationOptions,
) -> Result<EvalRecordObservation, WyrdError> {
    let (trace_id, span_id) = match options.trace_id {
        Some(trace_id) => (Some(trace_id), options.span_id),
        None if options.span_id.is_some() => (None, options.span_id),
        None => active_span_identity(),
    };
    let record = EvalRecordObservation {
        record_id: RecordId(uuid::Uuid::now_v7()),
        session_id: options.session_id,
        context,
        trace_id,
        span_id,
        created_at: Utc::now(),
        media: (!options.media.is_empty()).then_some(options.media),
    };
    record.validate()?;
    Ok(record)
}

/// Project the canonical Eval record into its one fixed-schema row.
///
/// `context` and `media` become canonical JSON text because the fixed table
/// declares them as scalar strings; trace and span identities become the
/// canonical lower-case hex the schema-driven row builder decodes into
/// `FixedSizeBinary`, matching `vala.traces.spans` byte for byte.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when `context` or a media
/// descriptor cannot be encoded as canonical JSON text.
pub(crate) fn row(record: &EvalRecordObservation) -> Result<Vec<u8>, WyrdError> {
    let context = json_text(&record.context, "context")?;
    let media = match &record.media {
        Some(media) => Value::String(json_text(media, "media")?),
        None => Value::Null,
    };
    let created_at = serde_json::to_value(record.created_at).map_err(|error| {
        invalid_observation(
            "created_at is not JSON",
            json!({ "source": error.to_string() }),
        )
    })?;
    row_bytes(&json!({
        "record_id": record.record_id.0.to_string(),
        "session_id": record.session_id.as_ref().map(|session| session.0.to_string()),
        "context": context,
        "trace_id": record.trace_id.as_ref().map(TraceId::to_hex),
        "span_id": record.span_id.as_ref().map(SpanId::to_hex),
        "created_at": created_at,
        "media": media,
    }))
}

/// Encode one nested value as the canonical JSON text a scalar column holds.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when the value is not
/// serializable, which for `context` includes a non-finite float.
fn json_text<T: serde::Serialize>(value: &T, field: &str) -> Result<String, WyrdError> {
    serde_json::to_string(value).map_err(|error| {
        invalid_observation(
            "eval observation field is not serializable as JSON",
            json!({ "field": field, "source": error.to_string() }),
        )
    })
}

/// Read trace and span identity from the active OpenTelemetry span, if any.
///
/// Returns both or neither: a span id is only meaningful inside its trace, and
/// the all-zero OTel sentinels are exactly what the typed constructors reject,
/// so an unsampled or absent span leaves the record's trace fields absent. No
/// process environment variable participates in this decision.
fn active_span_identity() -> (Option<TraceId>, Option<SpanId>) {
    let context = tracing::Span::current().context();
    let span = context.span();
    let span_context = span.span_context();
    if !span_context.is_valid() {
        return (None, None);
    }
    match (
        TraceId::from_bytes(span_context.trace_id().to_bytes()),
        SpanId::from_bytes(span_context.span_id().to_bytes()),
    ) {
        (Ok(trace_id), Ok(span_id)) => (Some(trace_id), Some(span_id)),
        _ => (None, None),
    }
}
