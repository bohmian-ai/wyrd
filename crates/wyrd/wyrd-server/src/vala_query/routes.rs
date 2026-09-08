//! Axum HTTP projection for `ValaQueryService` (task 16, HTTP surface).
//!
//! One handler per typed method, mounted under `/v1/{surface}/query` (see router()).
//! Each handler:
//!   1. Authorizes via `query::service::authorize_audited` (bifrost_query_read).
//!   2. Optionally verifies/issues a signed page token.
//!   3. Builds the LogicalPlan via `vala_query::service::build_*_plan`.
//!   4. Executes via `query::service::run_typed_query`.
//!   5. Extracts typed rows from the collected RecordBatches.
//!   6. Issues a next_page_token when has_more.
//!
//! Physical→API column mapping happens in `extract_*` helpers below.

use std::collections::HashMap;

use arrow::array::{
    Array, BinaryArray, FixedSizeBinaryArray, Float64Array, Int32Array, Int64Array, ListArray,
    RecordBatch, StringArray, StructArray, TimestampMicrosecondArray,
};
use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use chrono::{DateTime, Utc};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    AgentTraceRow, DriftRow, EvalRow, GenAiRow, GetTraceRequest, GetTraceResponse, LogRow,
    MAX_QUERY_PAGE_SIZE, MetricRow, QueryAgentTracesRequest, QueryAgentTracesResponse,
    QueryDriftRequest, QueryDriftResponse, QueryEvalRequest, QueryEvalResponse, QueryGenAiRequest,
    QueryGenAiResponse, QueryLogsRequest, QueryLogsResponse, QueryMetricsRequest,
    QueryMetricsResponse, QueryRecentTracesRequest, QueryRecentTracesResponse, QueryTracesRequest,
    QueryTracesResponse, SpanEventRow, SpanLinkRow, SpanRow, TraceSummaryRow, TraceWaterfall,
};

use crate::AppState;
use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::query::service::run_typed_query;
use crate::vala_query::page_token;
use crate::vala_query::payload;
use crate::vala_query::service::effective_limit;

/// Router for all typed ValaQuery HTTP routes (merged into `/v1`).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/traces/{trace_id}", get(get_trace))
        .route("/traces/query", post(query_traces))
        .route("/traces/recent/query", post(query_recent_traces))
        .route("/genai/query", post(query_genai))
        .route("/eval/query", post(query_eval))
        .route("/drift/query", post(query_drift))
        .route("/metrics/query", post(query_metrics))
        .route("/logs/query", post(query_logs))
        .route("/agent-traces/query", post(query_agent_traces))
}

// ─── hex helper ───────────────────────────────────────────────────────────────

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

// ─── timestamp helper ─────────────────────────────────────────────────────────

fn ts_us_to_dt(us: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_micros(us)
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).expect("epoch is valid"))
}

// ─── column accessors ─────────────────────────────────────────────────────────

fn col_ts<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a TimestampMicrosecondArray> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<TimestampMicrosecondArray>())
}

fn col_str<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a StringArray> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
}

fn col_i64<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a Int64Array> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
}

fn col_f64<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a Float64Array> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<Float64Array>())
}

fn col_i32<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a Int32Array> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
}

fn col_binary<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a BinaryArray> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<BinaryArray>())
}

fn col_list<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a ListArray> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<ListArray>())
}

fn col_fixed_binary<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a FixedSizeBinaryArray> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeBinaryArray>())
}

fn get_str(arr: Option<&StringArray>, i: usize) -> Option<&str> {
    arr.and_then(|a| if a.is_null(i) { None } else { Some(a.value(i)) })
}

fn get_hex(arr: Option<&FixedSizeBinaryArray>, i: usize) -> Option<String> {
    arr.and_then(|a| {
        if a.is_null(i) {
            None
        } else {
            Some(bytes_to_hex(a.value(i)))
        }
    })
}

fn get_json_str(arr: Option<&StringArray>, i: usize) -> Option<serde_json::Value> {
    arr.and_then(|a| if a.is_null(i) { None } else { Some(a.value(i)) })
        .and_then(|s| {
            serde_json::from_str(s)
                .ok()
                .or(Some(serde_json::Value::String(s.to_owned())))
        })
}

// ─── row extractors ───────────────────────────────────────────────────────────

/// Read one nullable `Int64` cell, treating an absent column as absent data.
fn get_i64(arr: Option<&Int64Array>, i: usize) -> Option<i64> {
    arr.filter(|a| !a.is_null(i)).map(|a| a.value(i))
}

/// Read one required `Int64` cell, defaulting an absent column to zero.
///
/// Every canonical count and nanosecond column is non-null, so a default is
/// only reachable when a metadata-only projection dropped the column.
fn req_i64(arr: Option<&Int64Array>, i: usize) -> i64 {
    get_i64(arr, i).unwrap_or_default()
}

/// Read one required `Utf8` cell, defaulting an absent column to the empty string.
fn req_str(arr: Option<&StringArray>, i: usize) -> String {
    get_str(arr, i).unwrap_or_default().to_owned()
}

/// Read one nullable canonical `Binary` payload cell.
///
/// `None` distinguishes "the plan did not project this column" — the shape an
/// unauthorized payload projection produces — from a present empty payload.
fn get_binary<'a>(arr: Option<&'a BinaryArray>, i: usize) -> Option<&'a [u8]> {
    arr.filter(|a| !a.is_null(i)).map(|a| a.value(i))
}

/// Project one canonical attribute column cell into public JSON.
///
/// # Errors
///
/// Propagates the stored-payload failures documented on
/// [`payload::attributes_to_json`].
fn attributes_json(
    arr: Option<&BinaryArray>,
    i: usize,
) -> Result<Option<serde_json::Value>, WyrdError> {
    get_binary(arr, i)
        .map(payload::attributes_to_json)
        .transpose()
}

/// Extract every span of one trace with its events and links nested on it.
///
/// The batches were produced by an already-authorized plan, so payload gating
/// shows up here as column absence: a metadata-only projection simply has no
/// `attributes`, `events`, `links`, `resource_attributes`, or
/// `scope_attributes` column, and the corresponding public fields stay `None`.
/// The `trace_id` filter is retained as defense in depth against a provider
/// returning a row the predicate should have excluded.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when a stored canonical payload column
/// cannot be decoded into the public JSON contract.
pub(crate) fn extract_span_rows_filtered(
    batches: &[RecordBatch],
    trace_id: &str,
) -> Result<Vec<SpanRow>, WyrdError> {
    let mut rows = Vec::new();
    for batch in batches {
        let trace_id_col = col_fixed_binary(batch, "trace_id");
        let span_id_col = col_fixed_binary(batch, "span_id");
        let parent_span_id_col = col_fixed_binary(batch, "parent_span_id");
        let trace_state_col = col_str(batch, "trace_state");
        let flags_col = col_i64(batch, "flags");
        let name_col = col_str(batch, "name");
        let kind_col = col_i32(batch, "kind");
        let start_col = col_i64(batch, "start_time_unix_nano");
        let end_col = col_i64(batch, "end_time_unix_nano");
        let duration_col = col_i64(batch, "duration_nano");
        let status_code_col = col_i32(batch, "status_code");
        let status_message_col = col_str(batch, "status_message");
        let attributes_col = col_binary(batch, "attributes");
        let dropped_attributes_col = col_i64(batch, "dropped_attributes_count");
        let events_col = col_list(batch, "events");
        let dropped_events_col = col_i64(batch, "dropped_events_count");
        let links_col = col_list(batch, "links");
        let dropped_links_col = col_i64(batch, "dropped_links_count");
        let service_name_col = col_str(batch, "service_name");
        let resource_attributes_col = col_binary(batch, "resource_attributes");
        let resource_dropped_col = col_i64(batch, "resource_dropped_attributes_count");
        let resource_schema_url_col = col_str(batch, "resource_schema_url");
        let scope_name_col = col_str(batch, "scope_name");
        let scope_version_col = col_str(batch, "scope_version");
        let scope_attributes_col = col_binary(batch, "scope_attributes");
        let scope_dropped_col = col_i64(batch, "scope_dropped_attributes_count");
        let scope_schema_url_col = col_str(batch, "scope_schema_url");

        for i in 0..batch.num_rows() {
            if get_hex(trace_id_col, i).as_deref() != Some(trace_id) {
                continue;
            }
            rows.push(SpanRow {
                span_id: get_hex(span_id_col, i).unwrap_or_default(),
                parent_span_id: get_hex(parent_span_id_col, i),
                trace_state: req_str(trace_state_col, i),
                flags: req_i64(flags_col, i),
                name: req_str(name_col, i),
                kind: kind_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| a.value(i))
                    .unwrap_or_default(),
                start_time_unix_nano: req_i64(start_col, i),
                end_time_unix_nano: req_i64(end_col, i),
                duration_nano: req_i64(duration_col, i),
                status_code: status_code_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| a.value(i)),
                status_message: get_str(status_message_col, i).map(ToOwned::to_owned),
                attributes: attributes_json(attributes_col, i)?,
                dropped_attributes_count: req_i64(dropped_attributes_col, i),
                events: extract_span_events(events_col, i)?,
                dropped_events_count: req_i64(dropped_events_col, i),
                links: extract_span_links(links_col, i)?,
                dropped_links_count: req_i64(dropped_links_col, i),
                service_name: get_str(service_name_col, i).map(ToOwned::to_owned),
                resource_attributes: attributes_json(resource_attributes_col, i)?,
                resource_dropped_attributes_count: req_i64(resource_dropped_col, i),
                resource_schema_url: req_str(resource_schema_url_col, i),
                scope_name: req_str(scope_name_col, i),
                scope_version: req_str(scope_version_col, i),
                scope_attributes: attributes_json(scope_attributes_col, i)?,
                scope_dropped_attributes_count: req_i64(scope_dropped_col, i),
                scope_schema_url: req_str(scope_schema_url_col, i),
            });
        }
    }
    Ok(rows)
}

/// Project one row's ordered span-event collection.
///
/// Returns `None` when the plan did not project the sensitive `events` column,
/// which is exactly the unauthorized case; a present empty list stays
/// `Some(vec![])` so "no events recorded" is never confused with "not permitted".
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the stored list does not carry the
/// declared nested struct or an event attribute payload is undecodable.
fn extract_span_events(
    list: Option<&ListArray>,
    row: usize,
) -> Result<Option<Vec<SpanEventRow>>, WyrdError> {
    let Some(values) = nested_struct(list, row)? else {
        return Ok(None);
    };
    let time_col = struct_i64(&values, "time_unix_nano")?;
    let name_col = struct_str(&values, "name")?;
    let attributes_col = struct_binary(&values, "attributes")?;
    let dropped_col = struct_i64(&values, "dropped_attributes_count")?;
    let mut events = Vec::with_capacity(values.len());
    for i in 0..values.len() {
        events.push(SpanEventRow {
            time_unix_nano: req_i64(Some(time_col), i),
            name: req_str(Some(name_col), i),
            attributes: attributes_json(Some(attributes_col), i)?,
            dropped_attributes_count: req_i64(Some(dropped_col), i),
        });
    }
    Ok(Some(events))
}

/// Project one row's ordered span-link collection.
///
/// Presence semantics match [`extract_span_events`].
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the stored list does not carry the
/// declared nested struct or a link attribute payload is undecodable.
fn extract_span_links(
    list: Option<&ListArray>,
    row: usize,
) -> Result<Option<Vec<SpanLinkRow>>, WyrdError> {
    let Some(values) = nested_struct(list, row)? else {
        return Ok(None);
    };
    let trace_id_col = struct_fixed_binary(&values, "trace_id")?;
    let span_id_col = struct_fixed_binary(&values, "span_id")?;
    let trace_state_col = struct_str(&values, "trace_state")?;
    let flags_col = struct_i64(&values, "flags")?;
    let attributes_col = struct_binary(&values, "attributes")?;
    let dropped_col = struct_i64(&values, "dropped_attributes_count")?;
    let mut links = Vec::with_capacity(values.len());
    for i in 0..values.len() {
        links.push(SpanLinkRow {
            linked_trace_id: get_hex(Some(trace_id_col), i).unwrap_or_default(),
            linked_span_id: get_hex(Some(span_id_col), i).unwrap_or_default(),
            trace_state: req_str(Some(trace_state_col), i),
            flags: req_i64(Some(flags_col), i),
            attributes: attributes_json(Some(attributes_col), i)?,
            dropped_attributes_count: req_i64(Some(dropped_col), i),
        });
    }
    Ok(Some(links))
}

/// Slice one row's list cell into the nested struct it declares.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the stored list element is not the
/// declared struct.
fn nested_struct(list: Option<&ListArray>, row: usize) -> Result<Option<StructArray>, WyrdError> {
    let Some(list) = list.filter(|a| !a.is_null(row)) else {
        return Ok(None);
    };
    let values = list.value(row);
    let structs = values
        .as_any()
        .downcast_ref::<StructArray>()
        .ok_or_else(|| stored_shape("nested span child is not a struct"))?;
    Ok(Some(structs.clone()))
}

/// Read one nested struct child as `Int64`.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the child is missing or mistyped.
fn struct_i64<'a>(values: &'a StructArray, name: &str) -> Result<&'a Int64Array, WyrdError> {
    values
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
        .ok_or_else(|| stored_shape("nested span child is missing an int64 field"))
}

/// Read one nested struct child as `Utf8`.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the child is missing or mistyped.
fn struct_str<'a>(values: &'a StructArray, name: &str) -> Result<&'a StringArray, WyrdError> {
    values
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| stored_shape("nested span child is missing a string field"))
}

/// Read one nested struct child as `Binary`.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the child is missing or mistyped.
fn struct_binary<'a>(values: &'a StructArray, name: &str) -> Result<&'a BinaryArray, WyrdError> {
    values
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<BinaryArray>())
        .ok_or_else(|| stored_shape("nested span child is missing a binary field"))
}

/// Read one nested struct child as fixed-width binary.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the child is missing or mistyped.
fn struct_fixed_binary<'a>(
    values: &'a StructArray,
    name: &str,
) -> Result<&'a FixedSizeBinaryArray, WyrdError> {
    values
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeBinaryArray>())
        .ok_or_else(|| stored_shape("nested span child is missing an id field"))
}

/// A stored canonical row does not carry the shape its table declares.
fn stored_shape(detail: &'static str) -> WyrdError {
    WyrdError::Internal {
        message: "stored canonical span does not match its declared schema".to_owned(),
        details: serde_json::json!({ "detail": detail }),
    }
}

/// Aggregate raw canonical span batches into trace summaries grouped by trace id.
///
/// Summaries are metadata only: they read the promoted `service_name`, the
/// nanosecond bounds, and the raw status discriminant, and never touch a
/// payload column.
pub(crate) fn aggregate_spans_to_summaries(batches: &[RecordBatch]) -> Vec<TraceSummaryRow> {
    /// Running per-trace aggregate over one collected span set.
    #[derive(Default)]
    struct Acc {
        /// Name of the trace's root span.
        root_name: String,
        /// Promoted service name of the root span, or the first span seen.
        service: String,
        /// Earliest span start in protocol nanoseconds.
        min_start_nanos: i64,
        /// Latest span end in protocol nanoseconds.
        max_end_nanos: i64,
        /// Number of spans observed for this trace.
        span_count: u32,
        /// Whether any span carried the OTLP `ERROR` status code.
        error: bool,
    }
    /// OTLP `Status.StatusCode.STATUS_CODE_ERROR`.
    const STATUS_CODE_ERROR: i32 = 2;

    let mut map: HashMap<String, Acc> = HashMap::new();

    for batch in batches {
        let trace_id_col = col_fixed_binary(batch, "trace_id");
        let name_col = col_str(batch, "name");
        let svc_col = col_str(batch, "service_name");
        let start_col = col_i64(batch, "start_time_unix_nano");
        let end_col = col_i64(batch, "end_time_unix_nano");
        let status_code_col = col_i32(batch, "status_code");
        let parent_col = col_fixed_binary(batch, "parent_span_id");

        for i in 0..batch.num_rows() {
            let trace_id = get_hex(trace_id_col, i).unwrap_or_default();
            let acc = map.entry(trace_id).or_default();

            let start_nanos = req_i64(start_col, i);
            let end_nanos = get_i64(end_col, i).unwrap_or(start_nanos);

            if acc.span_count == 0 {
                acc.min_start_nanos = start_nanos;
                acc.max_end_nanos = end_nanos;
            } else {
                acc.min_start_nanos = acc.min_start_nanos.min(start_nanos);
                acc.max_end_nanos = acc.max_end_nanos.max(end_nanos);
            }
            acc.span_count += 1;

            let is_root = parent_col.map(|a| a.is_null(i)).unwrap_or(false);
            if is_root {
                acc.root_name = req_str(name_col, i);
                acc.service = req_str(svc_col, i);
            } else if acc.service.is_empty() {
                acc.service = req_str(svc_col, i);
            }

            if status_code_col
                .filter(|a| !a.is_null(i))
                .map(|a| a.value(i))
                == Some(STATUS_CODE_ERROR)
            {
                acc.error = true;
            }
        }
    }

    let mut rows: Vec<TraceSummaryRow> = map
        .into_iter()
        .map(|(trace_id, acc)| TraceSummaryRow {
            trace_id,
            root_name: acc.root_name,
            service: acc.service,
            started_at: ts_us_to_dt(acc.min_start_nanos / 1_000),
            duration_ms: nanos_to_millis(acc.max_end_nanos - acc.min_start_nanos),
            span_count: acc.span_count,
            error: acc.error,
        })
        .collect();

    rows.sort_by_key(|b| std::cmp::Reverse(b.started_at));
    rows
}

/// Convert a non-negative nanosecond span into fractional milliseconds.
fn nanos_to_millis(nanos: i64) -> f64 {
    nanos.max(0) as f64 / 1_000_000.0
}

/// Extract GenAI generation rows from canonical span batches.
///
/// Every scalar is a promoted span column. The structured message payloads are
/// read out of the canonical `attributes` column, which the plan projects only
/// for a caller holding `BifrostGenAiPayload:Read`; without that column the
/// message fields stay `None` and no payload byte was read.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when a stored canonical attribute payload
/// cannot satisfy the public structured-JSON contract.
pub(crate) fn extract_genai_rows(batches: &[RecordBatch]) -> Result<Vec<GenAiRow>, WyrdError> {
    /// Canonical attribute key holding the ordered input messages.
    const INPUT_MESSAGES: &str = "gen_ai.input.messages";
    /// Canonical attribute key holding the ordered output messages.
    const OUTPUT_MESSAGES: &str = "gen_ai.output.messages";

    let mut rows = Vec::new();
    for batch in batches {
        let conv_col = col_str(batch, "gen_ai_conversation_id");
        let model_col = col_str(batch, "gen_ai_request_model");
        let prov_col = col_str(batch, "gen_ai_provider_name");
        let start_col = col_i64(batch, "start_time_unix_nano");
        let in_tok_col = col_i64(batch, "gen_ai_usage_input_tokens");
        let out_tok_col = col_i64(batch, "gen_ai_usage_output_tokens");
        let attributes_col = col_binary(batch, "attributes");

        for i in 0..batch.num_rows() {
            let (input_messages, output_messages) = match get_binary(attributes_col, i) {
                Some(bytes) => {
                    let attributes = payload::decode_attributes(bytes)?;
                    (
                        payload::structured_attribute(&attributes, INPUT_MESSAGES)?,
                        payload::structured_attribute(&attributes, OUTPUT_MESSAGES)?,
                    )
                }
                None => (None, None),
            };
            rows.push(GenAiRow {
                conversation_id: get_str(conv_col, i).map(ToOwned::to_owned),
                model: get_str(model_col, i).map(ToOwned::to_owned),
                provider: get_str(prov_col, i).map(ToOwned::to_owned),
                start_time_unix_nano: req_i64(start_col, i),
                input_tokens: get_i64(in_tok_col, i),
                output_tokens: get_i64(out_tok_col, i),
                input_messages,
                output_messages,
            });
        }
    }
    Ok(rows)
}

pub(crate) fn extract_eval_rows(batches: &[RecordBatch]) -> Vec<EvalRow> {
    let mut rows = Vec::new();
    for batch in batches {
        let eval_ref_col = col_str(batch, "eval_ref");
        let run_id_col = col_str(batch, "run_id");
        let metric_col = col_str(batch, "assertion_name");
        let score_col = col_f64(batch, "score_value");
        let start_col = col_ts(batch, "created_at");

        for i in 0..batch.num_rows() {
            rows.push(EvalRow {
                eval_id: get_str(eval_ref_col, i).unwrap_or("").to_owned(),
                run_id: get_str(run_id_col, i).unwrap_or("").to_owned(),
                metric: get_str(metric_col, i).unwrap_or("").to_owned(),
                score: score_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| a.value(i))
                    .unwrap_or(0.0),
                started_at: start_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| ts_us_to_dt(a.value(i)))
                    .unwrap_or_default(),
            });
        }
    }
    rows
}

pub(crate) fn extract_drift_rows(batches: &[RecordBatch]) -> Vec<DriftRow> {
    let mut rows = Vec::new();
    for batch in batches {
        let feat_col = col_str(batch, "series");
        let run_id_col = col_str(batch, "run_id");
        let score_col = col_f64(batch, "num_value");
        let ts_col = col_ts(batch, "created_at");

        for i in 0..batch.num_rows() {
            rows.push(DriftRow {
                feature: get_str(feat_col, i).unwrap_or("").to_owned(),
                run_id: get_str(run_id_col, i).map(|s| s.to_owned()),
                drift_score: score_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| a.value(i))
                    .unwrap_or(0.0),
                threshold: None,
                computed_at: ts_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| ts_us_to_dt(a.value(i)))
                    .unwrap_or_default(),
            });
        }
    }
    rows
}

pub(crate) fn extract_metric_rows(batches: &[RecordBatch]) -> Vec<MetricRow> {
    let mut rows = Vec::new();
    for batch in batches {
        let name_col = col_str(batch, "metric_name");
        let type_col = col_str(batch, "metric_type");
        let val_col = col_f64(batch, "value");
        let ts_col = col_ts(batch, "time");
        let attr_col = col_str(batch, "attributes");

        for i in 0..batch.num_rows() {
            rows.push(MetricRow {
                metric_name: get_str(name_col, i).unwrap_or("").to_owned(),
                metric_type: get_str(type_col, i).unwrap_or("").to_owned(),
                value: val_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| a.value(i))
                    .unwrap_or(0.0),
                timestamp: ts_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| ts_us_to_dt(a.value(i)))
                    .unwrap_or_default(),
                attributes: get_json_str(attr_col, i),
            });
        }
    }
    rows
}

pub(crate) fn extract_log_rows(batches: &[RecordBatch]) -> Vec<LogRow> {
    let mut rows = Vec::new();
    for batch in batches {
        let ts_col = col_ts(batch, "observed_time");
        // Severity numbers are stored as Int64 because Iceberg has no unsigned
        // integer type.
        let sev_num_col = col_i64(batch, "severity_number");
        let sev_txt_col = col_str(batch, "severity_text");
        let trace_col = col_fixed_binary(batch, "trace_id");
        let span_col = batch
            .column_by_name("span_id")
            .and_then(|c| c.as_any().downcast_ref::<FixedSizeBinaryArray>());
        let evt_col = col_str(batch, "event_name");
        let body_col = col_str(batch, "body");

        for i in 0..batch.num_rows() {
            rows.push(LogRow {
                timestamp: ts_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| ts_us_to_dt(a.value(i)))
                    .unwrap_or_default(),
                severity_number: sev_num_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| a.value(i) as i32)
                    .unwrap_or(0),
                severity_text: get_str(sev_txt_col, i).unwrap_or("").to_owned(),
                trace_id: get_hex(trace_col, i),
                span_id: get_hex(span_col, i),
                event_name: get_str(evt_col, i).map(|s| s.to_owned()),
                body: get_str(body_col, i).map(|s| s.to_owned()),
            });
        }
    }
    rows
}

pub(crate) fn extract_agent_trace_rows(batches: &[RecordBatch]) -> Vec<AgentTraceRow> {
    let mut rows = Vec::new();
    for batch in batches {
        let sess_col = col_str(batch, "dev_session_id");
        let repo_col = col_str(batch, "repo");
        let sha_col = col_str(batch, "commit_sha");
        let branch_col = col_str(batch, "branch");
        let run_col = col_str(batch, "run_id");
        let start_col = col_ts(batch, "started_at");
        let msgs_col = col_str(batch, "messages");

        for i in 0..batch.num_rows() {
            rows.push(AgentTraceRow {
                dev_session_id: get_str(sess_col, i).unwrap_or("").to_owned(),
                repo: get_str(repo_col, i).unwrap_or("").to_owned(),
                commit_sha: get_str(sha_col, i).map(|s| s.to_owned()),
                branch: get_str(branch_col, i).map(|s| s.to_owned()),
                run_id: get_str(run_col, i).map(|s| s.to_owned()),
                started_at: start_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| ts_us_to_dt(a.value(i)))
                    .unwrap_or_default(),
                payload: get_json_str(msgs_col, i),
            });
        }
    }
    rows
}

// ─── page-token helpers ───────────────────────────────────────────────────────

fn maybe_issue_token(
    has_more: bool,
    state: &AppState,
    caller: &Caller,
    route: &str,
    query_hash: u64,
) -> Option<String> {
    if !has_more {
        return None;
    }
    let key = super::sealing_key(state)?;
    let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
    let body = page_token::PageTokenBody::new(
        caller.data_tenant_id.to_string(),
        auth_hash,
        route,
        query_hash,
        String::new(),
        None,
        page_token::DEFAULT_PAGE_TOKEN_TTL_SECS,
    );
    page_token::encode(&body, key).ok()
}

/// Optional scan bounds carried as query parameters on the trace-detail route.
///
/// Trace detail returns one complete authorized cut, so these narrow the
/// `wyrd_event_time` scan only; there is no page size and no continuation
/// token on this contract.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct TraceDetailBounds {
    /// Inclusive lower bound on `wyrd_event_time`.
    #[serde(default)]
    since: Option<DateTime<Utc>>,
    /// Exclusive upper bound on `wyrd_event_time`.
    #[serde(default)]
    until: Option<DateTime<Utc>>,
}

// ─── handlers ────────────────────────────────────────────────────────────────

async fn get_trace(
    State(state): State<AppState>,
    caller: Caller,
    Path(trace_id): Path<String>,
    Query(bounds): Query<TraceDetailBounds>,
) -> Result<Json<GetTraceResponse>, WyrdErrorResponse> {
    use crate::vala_query::service::build_get_trace_plan;

    let req = GetTraceRequest {
        trace_id: trace_id.clone(),
        since: bounds.since,
        until: bounds.until,
    };
    let plan = build_get_trace_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, _) = run_typed_query(&state, &caller, plan, MAX_QUERY_PAGE_SIZE)
        .await
        .map_err(WyrdErrorResponse::from)?;

    // Keep the extraction filter as a defense-in-depth check for provider rows.
    let spans: Vec<SpanRow> =
        extract_span_rows_filtered(&batches, &trace_id).map_err(WyrdErrorResponse::from)?;

    if spans.is_empty() {
        let err: wyrd_spec::error::WyrdError = wyrd_spec::vala::BifrostError::TraceNotFound {
            trace_id: trace_id.clone(),
        }
        .into();
        return Err(WyrdErrorResponse::from(err));
    }

    Ok(Json(GetTraceResponse {
        trace: TraceWaterfall { trace_id, spans },
    }))
}

async fn query_traces(
    State(state): State<AppState>,
    caller: Caller,
    Json(req): Json<QueryTracesRequest>,
) -> Result<Json<QueryTracesResponse>, WyrdErrorResponse> {
    use crate::vala_query::service::build_query_traces_plan;

    let limit = effective_limit(req.window.limit);
    let qhash = page_token::query_hash(&[
        req.service.as_deref().unwrap_or(""),
        req.status.as_deref().unwrap_or(""),
        req.name.as_deref().unwrap_or(""),
    ]);

    if let Some(token) = &req.window.page_token
        && let Some(key) = super::sealing_key(&state)
    {
        let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
        let tenant_id_str = caller.data_tenant_id.to_string();
        page_token::verify(
            token,
            key,
            &tenant_id_str,
            &auth_hash,
            "query_traces",
            qhash,
        )
        .map_err(|e| {
            let err: wyrd_spec::error::WyrdError = e.into();
            WyrdErrorResponse::from(err)
        })?;
    }

    let plan = build_query_traces_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_typed_query(&state, &caller, plan, limit)
        .await
        .map_err(WyrdErrorResponse::from)?;

    let rows = aggregate_spans_to_summaries(&batches);
    let next_page_token = maybe_issue_token(has_more, &state, &caller, "query_traces", qhash);

    Ok(Json(QueryTracesResponse {
        rows,
        next_page_token,
    }))
}

async fn query_recent_traces(
    State(state): State<AppState>,
    caller: Caller,
    Json(req): Json<QueryRecentTracesRequest>,
) -> Result<Json<QueryRecentTracesResponse>, WyrdErrorResponse> {
    use crate::vala_query::service::build_query_recent_traces_plan;

    let limit = effective_limit(req.window.limit);
    let qhash = page_token::query_hash(&[
        req.service.as_deref().unwrap_or(""),
        req.status.as_deref().unwrap_or(""),
    ]);

    if let Some(token) = &req.window.page_token
        && let Some(key) = super::sealing_key(&state)
    {
        let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
        let tenant_id_str = caller.data_tenant_id.to_string();
        page_token::verify(
            token,
            key,
            &tenant_id_str,
            &auth_hash,
            "query_recent_traces",
            qhash,
        )
        .map_err(|e| {
            let err: wyrd_spec::error::WyrdError = e.into();
            WyrdErrorResponse::from(err)
        })?;
    }

    let plan = build_query_recent_traces_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_typed_query(&state, &caller, plan, limit)
        .await
        .map_err(WyrdErrorResponse::from)?;

    let mut rows = aggregate_spans_to_summaries(&batches);
    rows.truncate(limit as usize);

    let next_page_token =
        maybe_issue_token(has_more, &state, &caller, "query_recent_traces", qhash);
    Ok(Json(QueryRecentTracesResponse {
        rows,
        next_page_token,
    }))
}

async fn query_genai(
    State(state): State<AppState>,
    caller: Caller,
    Json(req): Json<QueryGenAiRequest>,
) -> Result<Json<QueryGenAiResponse>, WyrdErrorResponse> {
    use crate::vala_query::service::build_query_genai_plan;

    let limit = effective_limit(req.window.limit);
    let qhash = page_token::query_hash(&[
        req.conversation_id.as_deref().unwrap_or(""),
        req.model.as_deref().unwrap_or(""),
        req.provider.as_deref().unwrap_or(""),
    ]);

    if let Some(token) = &req.window.page_token
        && let Some(key) = super::sealing_key(&state)
    {
        let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
        let tenant_id_str = caller.data_tenant_id.to_string();
        page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_genai", qhash).map_err(
            |e| {
                let err: wyrd_spec::error::WyrdError = e.into();
                WyrdErrorResponse::from(err)
            },
        )?;
    }

    let plan = build_query_genai_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_typed_query(&state, &caller, plan, limit)
        .await
        .map_err(WyrdErrorResponse::from)?;

    let rows = extract_genai_rows(&batches).map_err(WyrdErrorResponse::from)?;
    let next_page_token = maybe_issue_token(has_more, &state, &caller, "query_genai", qhash);
    Ok(Json(QueryGenAiResponse {
        rows,
        next_page_token,
    }))
}

async fn query_eval(
    State(state): State<AppState>,
    caller: Caller,
    Json(req): Json<QueryEvalRequest>,
) -> Result<Json<QueryEvalResponse>, WyrdErrorResponse> {
    use crate::vala_query::service::build_query_eval_plan;

    let limit = effective_limit(req.window.limit);
    let qhash = page_token::query_hash(&[
        req.eval_id.as_deref().unwrap_or(""),
        req.run_id.as_deref().unwrap_or(""),
    ]);

    if let Some(token) = &req.window.page_token
        && let Some(key) = super::sealing_key(&state)
    {
        let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
        let tenant_id_str = caller.data_tenant_id.to_string();
        page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_eval", qhash).map_err(
            |e| {
                let err: wyrd_spec::error::WyrdError = e.into();
                WyrdErrorResponse::from(err)
            },
        )?;
    }

    let plan = build_query_eval_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_typed_query(&state, &caller, plan, limit)
        .await
        .map_err(WyrdErrorResponse::from)?;

    let rows = extract_eval_rows(&batches);
    let next_page_token = maybe_issue_token(has_more, &state, &caller, "query_eval", qhash);
    Ok(Json(QueryEvalResponse {
        rows,
        next_page_token,
    }))
}

async fn query_drift(
    State(state): State<AppState>,
    caller: Caller,
    Json(req): Json<QueryDriftRequest>,
) -> Result<Json<QueryDriftResponse>, WyrdErrorResponse> {
    use crate::vala_query::service::build_query_drift_plan;

    let limit = effective_limit(req.window.limit);
    let qhash = page_token::query_hash(&[
        req.feature.as_deref().unwrap_or(""),
        req.run_id.as_deref().unwrap_or(""),
    ]);

    if let Some(token) = &req.window.page_token
        && let Some(key) = super::sealing_key(&state)
    {
        let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
        let tenant_id_str = caller.data_tenant_id.to_string();
        page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_drift", qhash).map_err(
            |e| {
                let err: wyrd_spec::error::WyrdError = e.into();
                WyrdErrorResponse::from(err)
            },
        )?;
    }

    let plan = build_query_drift_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_typed_query(&state, &caller, plan, limit)
        .await
        .map_err(WyrdErrorResponse::from)?;

    let rows = extract_drift_rows(&batches);
    let next_page_token = maybe_issue_token(has_more, &state, &caller, "query_drift", qhash);
    Ok(Json(QueryDriftResponse {
        rows,
        next_page_token,
    }))
}

async fn query_metrics(
    State(state): State<AppState>,
    caller: Caller,
    Json(req): Json<QueryMetricsRequest>,
) -> Result<Json<QueryMetricsResponse>, WyrdErrorResponse> {
    use crate::vala_query::service::build_query_metrics_plan;

    let limit = effective_limit(req.window.limit);
    let qhash = page_token::query_hash(&[
        req.metric_name.as_deref().unwrap_or(""),
        req.metric_type.as_deref().unwrap_or(""),
    ]);

    if let Some(token) = &req.window.page_token
        && let Some(key) = super::sealing_key(&state)
    {
        let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
        let tenant_id_str = caller.data_tenant_id.to_string();
        page_token::verify(
            token,
            key,
            &tenant_id_str,
            &auth_hash,
            "query_metrics",
            qhash,
        )
        .map_err(|e| {
            let err: wyrd_spec::error::WyrdError = e.into();
            WyrdErrorResponse::from(err)
        })?;
    }

    let plan = build_query_metrics_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_typed_query(&state, &caller, plan, limit)
        .await
        .map_err(WyrdErrorResponse::from)?;

    let rows = extract_metric_rows(&batches);
    let next_page_token = maybe_issue_token(has_more, &state, &caller, "query_metrics", qhash);
    Ok(Json(QueryMetricsResponse {
        rows,
        next_page_token,
    }))
}

async fn query_logs(
    State(state): State<AppState>,
    caller: Caller,
    Json(req): Json<QueryLogsRequest>,
) -> Result<Json<QueryLogsResponse>, WyrdErrorResponse> {
    use crate::vala_query::service::build_query_logs_plan;

    let limit = effective_limit(req.window.limit);
    let qhash = page_token::query_hash(&[req.trace_id.as_deref().unwrap_or("")]);

    if let Some(token) = &req.window.page_token
        && let Some(key) = super::sealing_key(&state)
    {
        let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
        let tenant_id_str = caller.data_tenant_id.to_string();
        page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_logs", qhash).map_err(
            |e| {
                let err: wyrd_spec::error::WyrdError = e.into();
                WyrdErrorResponse::from(err)
            },
        )?;
    }

    let plan = build_query_logs_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_typed_query(&state, &caller, plan, limit)
        .await
        .map_err(WyrdErrorResponse::from)?;

    let rows = extract_log_rows(&batches);
    let next_page_token = maybe_issue_token(has_more, &state, &caller, "query_logs", qhash);
    Ok(Json(QueryLogsResponse {
        rows,
        next_page_token,
    }))
}

async fn query_agent_traces(
    State(state): State<AppState>,
    caller: Caller,
    Json(req): Json<QueryAgentTracesRequest>,
) -> Result<Json<QueryAgentTracesResponse>, WyrdErrorResponse> {
    use crate::vala_query::service::build_query_agent_traces_plan;

    let limit = effective_limit(req.window.limit);
    let qhash = page_token::query_hash(&[
        req.dev_session_id.as_deref().unwrap_or(""),
        req.repo.as_deref().unwrap_or(""),
        req.run_id.as_deref().unwrap_or(""),
    ]);

    if let Some(token) = &req.window.page_token
        && let Some(key) = super::sealing_key(&state)
    {
        let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
        let tenant_id_str = caller.data_tenant_id.to_string();
        page_token::verify(
            token,
            key,
            &tenant_id_str,
            &auth_hash,
            "query_agent_traces",
            qhash,
        )
        .map_err(|e| {
            let err: wyrd_spec::error::WyrdError = e.into();
            WyrdErrorResponse::from(err)
        })?;
    }

    let plan = build_query_agent_traces_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_typed_query(&state, &caller, plan, limit)
        .await
        .map_err(WyrdErrorResponse::from)?;

    let rows = extract_agent_trace_rows(&batches);
    let next_page_token = maybe_issue_token(has_more, &state, &caller, "query_agent_traces", qhash);
    Ok(Json(QueryAgentTracesResponse {
        rows,
        next_page_token,
    }))
}
