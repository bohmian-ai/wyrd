//! Axum HTTP projection for `ValaQueryService` (task 16, HTTP surface).
//!
//! One handler per typed method, mounted under `/v1/{surface}/query` (see router()).
//! Each handler:
//!   1. Authorizes via `query::service::authorize_audited` (bifrost_query_read).
//!   2. Optionally verifies/issues a signed page token.
//!   3. Builds the LogicalPlan via `vala_query::service::build_*_plan`.
//!   4. Executes via `query::service::run_plan_query`.
//!   5. Extracts typed rows from the collected RecordBatches.
//!   6. Issues a next_page_token when has_more.
//!
//! Physical→API column mapping happens in `extract_*` helpers below.

use std::collections::HashMap;

use arrow::array::{
    Array, FixedSizeBinaryArray, Float64Array, RecordBatch, StringArray, StringViewArray,
    TimestampMicrosecondArray, UInt32Array, UInt64Array,
};
use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use chrono::{DateTime, Utc};
use wyrd_spec::vala::api::{
    AgentTraceRow, DriftRow, EvalRow, GenAiRow, GetTraceRequest, GetTraceResponse, LogRow,
    MetricRow, QueryAgentTracesRequest, QueryAgentTracesResponse, QueryDriftRequest,
    QueryDriftResponse, QueryEvalRequest, QueryEvalResponse, QueryGenAiRequest, QueryGenAiResponse,
    QueryLogsRequest, QueryLogsResponse, QueryMetricsRequest, QueryMetricsResponse,
    QueryRecentTracesRequest, QueryRecentTracesResponse, QueryTracesRequest, QueryTracesResponse,
    SpanRow, TraceSummaryRow, TraceWaterfall,
};

use crate::AppState;
use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::query::service::run_plan_query;
use crate::vala_query::page_token;
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

fn col_u32<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a UInt32Array> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<UInt32Array>())
}

fn col_u64<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a UInt64Array> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<UInt64Array>())
}

fn col_f64<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a Float64Array> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<Float64Array>())
}

fn col_bin16<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a FixedSizeBinaryArray> {
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

fn col_str_view<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a StringViewArray> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<StringViewArray>())
}

fn get_str_view<'a>(arr: Option<&'a StringViewArray>, i: usize) -> Option<&'a str> {
    arr.and_then(|a| if a.is_null(i) { None } else { Some(a.value(i)) })
}

fn get_json_str_view(arr: Option<&StringViewArray>, i: usize) -> Option<serde_json::Value> {
    arr.and_then(|a| {
        if a.is_null(i) {
            None
        } else {
            let s = a.value(i);
            serde_json::from_str(s)
                .ok()
                .or(Some(serde_json::Value::String(s.to_owned())))
        }
    })
}

// ─── row extractors ───────────────────────────────────────────────────────────

pub(crate) fn extract_span_rows_filtered(batches: &[RecordBatch], trace_id: &str) -> Vec<SpanRow> {
    extract_span_rows_impl(batches, Some(trace_id))
}

fn extract_span_rows_impl(batches: &[RecordBatch], trace_id_filter: Option<&str>) -> Vec<SpanRow> {
    let mut rows = Vec::new();
    for batch in batches {
        let trace_id_col = col_bin16(batch, "trace_id");
        let span_id_col = col_bin16(batch, "span_id");
        let parent_span_id_col = col_bin16(batch, "parent_span_id");
        let name_col = col_str(batch, "name");
        let kind_col = col_str(batch, "kind");
        let start_col = col_ts(batch, "start_time");
        let dur_col = col_u64(batch, "duration_ms");
        let status_col = col_str(batch, "status");
        let attr_col = col_str_view(batch, "attributes");

        for i in 0..batch.num_rows() {
            if let Some(filter) = trace_id_filter {
                let row_trace_id = get_hex(trace_id_col, i);
                if row_trace_id.as_deref() != Some(filter) {
                    continue;
                }
            }
            rows.push(SpanRow {
                span_id: get_hex(span_id_col, i).unwrap_or_default(),
                parent_span_id: get_hex(parent_span_id_col, i),
                name: get_str(name_col, i).unwrap_or("").to_owned(),
                kind: get_str(kind_col, i).unwrap_or("").to_owned(),
                started_at: start_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| ts_us_to_dt(a.value(i)))
                    .unwrap_or_default(),
                duration_ms: dur_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| a.value(i) as f64)
                    .unwrap_or(0.0),
                status: get_str(status_col, i).unwrap_or("").to_owned(),
                attributes: get_json_str_view(attr_col, i),
            });
        }
    }
    rows
}

/// Aggregate raw span batches into trace summaries grouped by trace_id.
pub(crate) fn aggregate_spans_to_summaries(batches: &[RecordBatch]) -> Vec<TraceSummaryRow> {
    #[derive(Default)]
    struct Acc {
        root_name: String,
        service: String,
        min_start_us: i64,
        max_end_us: i64,
        span_count: u32,
        error: bool,
    }
    let mut map: HashMap<String, Acc> = HashMap::new();

    for batch in batches {
        let trace_id_col = col_bin16(batch, "trace_id");
        let name_col = col_str(batch, "name");
        let svc_col = col_str(batch, "service_name");
        let start_col = col_ts(batch, "start_time");
        let end_col = col_ts(batch, "end_time");
        let status_col = col_str(batch, "status");
        let parent_col = col_bin16(batch, "parent_span_id");

        for i in 0..batch.num_rows() {
            let trace_id = get_hex(trace_id_col, i).unwrap_or_default();
            let acc = map.entry(trace_id).or_default();

            let start_us = start_col
                .filter(|a| !a.is_null(i))
                .map(|a| a.value(i))
                .unwrap_or(0);
            let end_us = end_col
                .filter(|a| !a.is_null(i))
                .map(|a| a.value(i))
                .unwrap_or(start_us);

            // Initialize or update min/max
            if acc.span_count == 0 {
                acc.min_start_us = start_us;
                acc.max_end_us = end_us;
            } else {
                if start_us < acc.min_start_us {
                    acc.min_start_us = start_us;
                }
                if end_us > acc.max_end_us {
                    acc.max_end_us = end_us;
                }
            }
            acc.span_count += 1;

            // Root span: parent_span_id IS NULL
            let is_root = parent_col.map(|a| a.is_null(i)).unwrap_or(false);
            if is_root {
                acc.root_name = get_str(name_col, i).unwrap_or("").to_owned();
                acc.service = get_str(svc_col, i).unwrap_or("").to_owned();
            } else if acc.service.is_empty() {
                acc.service = get_str(svc_col, i).unwrap_or("").to_owned();
            }

            if get_str(status_col, i) == Some("ERROR") {
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
            started_at: ts_us_to_dt(acc.min_start_us),
            duration_ms: (acc.max_end_us - acc.min_start_us).max(0) as f64 / 1000.0,
            span_count: acc.span_count,
            error: acc.error,
        })
        .collect();

    rows.sort_by_key(|b| std::cmp::Reverse(b.started_at));
    rows
}

pub(crate) fn extract_genai_rows(batches: &[RecordBatch]) -> Vec<GenAiRow> {
    let mut rows = Vec::new();
    for batch in batches {
        let conv_col = col_str(batch, "conversation_id");
        let model_col = col_str(batch, "request_model");
        let prov_col = col_str(batch, "provider_name");
        let start_col = col_ts(batch, "start_time");
        let in_tok_col = col_u32(batch, "usage_input_tokens");
        let out_tok_col = col_u32(batch, "usage_output_tokens");
        let prompt_col = col_str_view(batch, "input_messages");
        let completion_col = col_str_view(batch, "output_messages");

        for i in 0..batch.num_rows() {
            rows.push(GenAiRow {
                conversation_id: get_str(conv_col, i).unwrap_or("").to_owned(),
                model: get_str(model_col, i).unwrap_or("").to_owned(),
                provider: get_str(prov_col, i).unwrap_or("").to_owned(),
                started_at: start_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| ts_us_to_dt(a.value(i)))
                    .unwrap_or_default(),
                input_tokens: in_tok_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| a.value(i) as i64),
                output_tokens: out_tok_col
                    .filter(|a| !a.is_null(i))
                    .map(|a| a.value(i) as i64),
                cost_usd: None, // Stage N placeholder — no physical column in genai.messages yet
                prompt: get_str_view(prompt_col, i).map(|s| s.to_owned()),
                completion: get_str_view(completion_col, i).map(|s| s.to_owned()),
            });
        }
    }
    rows
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
        let attr_col = col_str_view(batch, "attributes");

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
                attributes: get_json_str_view(attr_col, i),
            });
        }
    }
    rows
}

pub(crate) fn extract_log_rows(batches: &[RecordBatch]) -> Vec<LogRow> {
    let mut rows = Vec::new();
    for batch in batches {
        let ts_col = col_ts(batch, "observed_time");
        let sev_num_col = col_u32(batch, "severity_number");
        let sev_txt_col = col_str(batch, "severity_text");
        let trace_col = col_bin16(batch, "trace_id");
        let span_col = batch
            .column_by_name("span_id")
            .and_then(|c| c.as_any().downcast_ref::<FixedSizeBinaryArray>());
        let evt_col = col_str(batch, "event_name");
        let body_col = col_str_view(batch, "body");

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
                body: get_str_view(body_col, i).map(|s| s.to_owned()),
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
        let msgs_col = col_str_view(batch, "messages");

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
                payload: get_json_str_view(msgs_col, i),
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

// ─── handlers ────────────────────────────────────────────────────────────────

async fn get_trace(
    State(state): State<AppState>,
    caller: Caller,
    Path(trace_id): Path<String>,
) -> Result<Json<GetTraceResponse>, WyrdErrorResponse> {
    use crate::vala_query::service::build_get_trace_plan;
    use wyrd_spec::vala::api::QueryWindow;

    let req = GetTraceRequest {
        window: QueryWindow {
            since: None,
            until: None,
            limit: None,
            // TODO(Stage 5): accept since/until as optional query parameters. Window is
            // hardcoded to the default 7-day scan for Stage 4.
            page_token: None,
        },
        trace_id: trace_id.clone(),
    };
    let plan = build_get_trace_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let limit = effective_limit(req.window.limit);
    let (batches, _) = run_plan_query(&state, &caller, plan, "vala.query.typed.get_trace", limit)
        .await
        .map_err(WyrdErrorResponse::from)?;

    // trace_id binary pushdown deferred to Stage 5 — Rust post-filter applied.
    let spans: Vec<SpanRow> = extract_span_rows_filtered(&batches, &trace_id);

    if spans.is_empty() {
        let err: wyrd_spec::error::WyrdError = wyrd_spec::vala::BifrostError::TraceNotFound {
            trace_id: trace_id.clone(),
        }
        .into();
        return Err(WyrdErrorResponse::from(err));
    }

    Ok(Json(GetTraceResponse {
        trace: TraceWaterfall {
            trace_id,
            spans,
            events: vec![],
            links: vec![],
        },
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

    if let Some(token) = &req.window.page_token {
        if let Some(key) = super::sealing_key(&state) {
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
    }

    let plan = build_query_traces_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_plan_query(
        &state,
        &caller,
        plan,
        "vala.query.typed.query_traces",
        limit,
    )
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

    if let Some(token) = &req.window.page_token {
        if let Some(key) = super::sealing_key(&state) {
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
    }

    let plan = build_query_recent_traces_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_plan_query(
        &state,
        &caller,
        plan,
        "vala.query.typed.query_recent_traces",
        limit,
    )
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

    if let Some(token) = &req.window.page_token {
        if let Some(key) = super::sealing_key(&state) {
            let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
            let tenant_id_str = caller.data_tenant_id.to_string();
            page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_genai", qhash)
                .map_err(|e| {
                    let err: wyrd_spec::error::WyrdError = e.into();
                    WyrdErrorResponse::from(err)
                })?;
        }
    }

    let plan = build_query_genai_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) =
        run_plan_query(&state, &caller, plan, "vala.query.typed.query_genai", limit)
            .await
            .map_err(WyrdErrorResponse::from)?;

    let rows = extract_genai_rows(&batches);
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

    if let Some(token) = &req.window.page_token {
        if let Some(key) = super::sealing_key(&state) {
            let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
            let tenant_id_str = caller.data_tenant_id.to_string();
            page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_eval", qhash)
                .map_err(|e| {
                    let err: wyrd_spec::error::WyrdError = e.into();
                    WyrdErrorResponse::from(err)
                })?;
        }
    }

    let plan = build_query_eval_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) =
        run_plan_query(&state, &caller, plan, "vala.query.typed.query_eval", limit)
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

    if let Some(token) = &req.window.page_token {
        if let Some(key) = super::sealing_key(&state) {
            let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
            let tenant_id_str = caller.data_tenant_id.to_string();
            page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_drift", qhash)
                .map_err(|e| {
                    let err: wyrd_spec::error::WyrdError = e.into();
                    WyrdErrorResponse::from(err)
                })?;
        }
    }

    let plan = build_query_drift_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) =
        run_plan_query(&state, &caller, plan, "vala.query.typed.query_drift", limit)
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

    if let Some(token) = &req.window.page_token {
        if let Some(key) = super::sealing_key(&state) {
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
    }

    let plan = build_query_metrics_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_plan_query(
        &state,
        &caller,
        plan,
        "vala.query.typed.query_metrics",
        limit,
    )
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

    if let Some(token) = &req.window.page_token {
        if let Some(key) = super::sealing_key(&state) {
            let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
            let tenant_id_str = caller.data_tenant_id.to_string();
            page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_logs", qhash)
                .map_err(|e| {
                    let err: wyrd_spec::error::WyrdError = e.into();
                    WyrdErrorResponse::from(err)
                })?;
        }
    }

    let plan = build_query_logs_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) =
        run_plan_query(&state, &caller, plan, "vala.query.typed.query_logs", limit)
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

    if let Some(token) = &req.window.page_token {
        if let Some(key) = super::sealing_key(&state) {
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
    }

    let plan = build_query_agent_traces_plan(&state, &caller, &req)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let (batches, has_more) = run_plan_query(
        &state,
        &caller,
        plan,
        "vala.query.typed.query_agent_traces",
        limit,
    )
    .await
    .map_err(WyrdErrorResponse::from)?;

    let rows = extract_agent_trace_rows(&batches);
    let next_page_token = maybe_issue_token(has_more, &state, &caller, "query_agent_traces", qhash);
    Ok(Json(QueryAgentTracesResponse {
        rows,
        next_page_token,
    }))
}
