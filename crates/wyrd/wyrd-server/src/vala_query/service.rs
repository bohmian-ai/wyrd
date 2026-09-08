//! Typed LogicalPlan builders for each ValaQueryService method (M-07).
//!
//! Each function:
//!   1. Requests a tenant-qualified typed DataFrame from retained Oracle.
//!   3. Applies typed filters as bound `Expr` literals — never string interpolation.
//!   4. Projects away sensitive payload columns when the caller lacks the payload permission.
//!   5. Returns the `LogicalPlan` (pre-analysis). The `TenantPredicateRule` analyzer
//!      injects the tenant predicate during Oracle's typed-plan execution.
//!
//! Note on trace-summary methods (QueryTraces, QueryRecentTraces): these return spans
//! plans filtered but NOT aggregated. The route handler performs the in-Rust GROUP-BY
//! aggregation after collecting batches. Stage 5 will push aggregation into the plan.
//!
//! Physical→API schema mapping:
//!   - Trace IDs are converted to fixed-binary literals for exact plan filtering;
//!     span IDs remain extraction-only fields.
//!   - String column name differences (start_time vs started_at, service_name vs service)
//!     are resolved in the extraction layer in routes.rs.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use datafusion::logical_expr::LogicalPlan;
use datafusion::prelude::{DataFrame, col, lit};
use datafusion::scalar::ScalarValue;
use wyrd_runtime::{Action, Permission, Resource};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    GetTraceRequest, MAX_QUERY_PAGE_SIZE, QueryAgentTracesRequest, QueryDriftRequest,
    QueryEvalRequest, QueryGenAiRequest, QueryLogsRequest, QueryMetricsRequest,
    QueryRecentTracesRequest, QueryTracesRequest,
};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

use crate::AppState;
use crate::components::auth::Caller;

/// Default time window for `GetTrace` and listing queries (F-08: no unbounded scans).
pub(crate) const DEFAULT_WINDOW_DAYS: i64 = 7;

/// Ceiling on records collected before pagination (mirrors MAX_QUERY_PAGE_SIZE).
pub(crate) fn effective_limit(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(MAX_QUERY_PAGE_SIZE)
        .min(MAX_QUERY_PAGE_SIZE)
}

/// Resolves one typed published table through the retained Oracle owner.
///
/// # Errors
///
/// Returns role-unavailable or a stable Oracle catalog/planning error.
async fn typed_dataframe(
    state: &AppState,
    caller: &Caller,
    audit_operation: &'static str,
    fqn: &str,
) -> Result<DataFrame, WyrdError> {
    crate::query::service::authorize_audited(
        state.clone(),
        caller.clone(),
        Permission::bifrost_query_read(),
        audit_operation,
        "vala.query",
    )
    .await?;
    state
        .bifrost_query()
        .ok_or(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable)?
        .oracle()
        .typed_dataframe(caller.data_tenant_id, fqn)
        .await
        .map_err(Into::into)
}

/// Apply a mandatory `wyrd_event_time` window filter. `since` defaults to
/// `now - DEFAULT_WINDOW_DAYS` when absent (F-08).
fn apply_window(
    df: DataFrame,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> Result<DataFrame, WyrdError> {
    let effective_since = since.unwrap_or_else(|| Utc::now() - Duration::days(DEFAULT_WINDOW_DAYS));
    let ts_since = ScalarValue::TimestampMicrosecond(
        Some(effective_since.timestamp_micros()),
        Some(Arc::from("UTC")),
    );
    let df = df
        .filter(col(WYRD_EVENT_TIME).gt_eq(lit(ts_since)))
        .map_err(df_err)?;
    if let Some(until_ts) = until {
        let ts_until = ScalarValue::TimestampMicrosecond(
            Some(until_ts.timestamp_micros()),
            Some(Arc::from("UTC")),
        );
        return df
            .filter(col(WYRD_EVENT_TIME).lt(lit(ts_until)))
            .map_err(df_err);
    }
    Ok(df)
}

fn df_err(e: datafusion::error::DataFusionError) -> WyrdError {
    WyrdError::Internal {
        message: "plan construction error".to_owned(),
        details: serde_json::json!({ "detail": e.to_string() }),
    }
}

fn opt_filter_str(
    df: DataFrame,
    column: &str,
    value: &Option<String>,
) -> Result<DataFrame, WyrdError> {
    match value {
        Some(v) => df.filter(col(column).eq(lit(v.clone()))).map_err(df_err),
        None => Ok(df),
    }
}

/// Filter spans by the textual OTLP status name used by the public contract.
///
/// The canonical column is the raw `status_code` discriminant, so the request's
/// `"UNSET"`/`"OK"`/`"ERROR"` text is mapped to that discriminant here rather
/// than compared against a stored string. An unrecognized name is a client
/// error, never a silently empty result.
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] for an unrecognized status name, or a
/// plan-construction error.
fn opt_filter_status(df: DataFrame, value: &Option<String>) -> Result<DataFrame, WyrdError> {
    let Some(name) = value else { return Ok(df) };
    let code = match name.to_ascii_uppercase().as_str() {
        "UNSET" => 0,
        "OK" => 1,
        "ERROR" => 2,
        _ => {
            return Err(WyrdError::Validation {
                message: "status must be UNSET, OK, or ERROR".to_owned(),
                details: serde_json::json!({ "status": name }),
            });
        }
    };
    df.filter(col("status_code").eq(lit(code))).map_err(df_err)
}

/// Filter spans by a minimum duration expressed in milliseconds.
///
/// The canonical column is `duration_nano`, so the requested millisecond bound
/// is widened to nanoseconds instead of compared against a derived column.
///
/// # Errors
///
/// Returns a plan-construction error.
fn opt_filter_min_duration(df: DataFrame, value: Option<u32>) -> Result<DataFrame, WyrdError> {
    let Some(millis) = value else { return Ok(df) };
    let nanos = i64::from(millis) * 1_000_000;
    df.filter(col("duration_nano").gt_eq(lit(nanos)))
        .map_err(df_err)
}

fn opt_filter_i32(df: DataFrame, column: &str, value: Option<i32>) -> Result<DataFrame, WyrdError> {
    match value {
        Some(v) => df.filter(col(column).gt_eq(lit(v))).map_err(df_err),
        None => Ok(df),
    }
}

fn has_perm(caller: &Caller, resource: Resource) -> bool {
    caller
        .principal
        .effective_permissions
        .contains(&Permission {
            resource,
            action: Action::Read,
        })
}

/// Sensitive span payload columns a trace read may project only with
/// `BifrostTracePayload:Read`.
///
/// This is the span table's declared sensitive set minus
/// `resource_entity_refs`, which no public trace contract returns and which is
/// therefore dropped from every trace plan.
const SPAN_PAYLOAD_COLUMNS: [&str; 5] = [
    "attributes",
    "events",
    "links",
    "resource_attributes",
    "scope_attributes",
];

/// Span columns no public trace contract returns, dropped from every plan.
const SPAN_UNPROJECTED_COLUMNS: [&str; 1] = ["resource_entity_refs"];

/// Build the `LogicalPlan` for `GetTrace` — one complete authorized trace cut.
///
/// The plan filters `vala.traces.spans` to the requested trace inside the
/// requested `wyrd_event_time` bounds and returns every matching span. The
/// sensitive payload columns are dropped from the projection unless the caller
/// holds `BifrostTracePayload:Read`, so an unauthorized plan never reads
/// attributes, events, links, or resource/scope payload from storage.
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] when `trace_id` is not exactly 16
/// hexadecimal bytes or `since` is later than `until`, the stable authorization
/// or audit error from [`typed_dataframe`], or a plan-construction error.
pub async fn build_get_trace_plan(
    state: &AppState,
    caller: &Caller,
    req: &GetTraceRequest,
) -> Result<LogicalPlan, WyrdError> {
    validate_bounds(req.since, req.until)?;
    let trace_id = trace_id_bytes(&req.trace_id)?;
    let df = typed_dataframe(
        state,
        caller,
        "vala.query.typed.get_trace",
        "vala.traces.spans",
    )
    .await?;
    let df = apply_window(df, req.since, req.until)?;
    let df = df
        .filter(col("trace_id").eq(lit(ScalarValue::FixedSizeBinary(16, Some(trace_id)))))
        .map_err(df_err)?;
    let df = df.drop_columns(&SPAN_UNPROJECTED_COLUMNS).map_err(df_err)?;
    let df = if has_perm(caller, Resource::BifrostTracePayload) {
        df
    } else {
        df.drop_columns(&SPAN_PAYLOAD_COLUMNS).map_err(df_err)?
    };

    Ok(df.logical_plan().clone())
}

/// Reject a scan window whose lower bound is later than its upper bound.
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] when both bounds are present and `since`
/// is strictly later than `until`.
fn validate_bounds(
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> Result<(), WyrdError> {
    if let (Some(since), Some(until)) = (since, until)
        && since > until
    {
        return Err(WyrdError::Validation {
            message: "since must not be later than until".to_owned(),
            details: serde_json::json!({
                "since": since.to_rfc3339(),
                "until": until.to_rfc3339(),
            }),
        });
    }
    Ok(())
}

/// Decode one hexadecimal trace id into its exact 16 protocol bytes.
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] when the text is not hexadecimal or does
/// not decode to exactly 16 bytes.
fn trace_id_bytes(trace_id: &str) -> Result<Vec<u8>, WyrdError> {
    let bytes = hex::decode(trace_id).map_err(|error| WyrdError::Validation {
        message: "trace_id must be a hexadecimal value".to_owned(),
        details: serde_json::json!({ "trace_id": trace_id, "detail": error.to_string() }),
    })?;
    if bytes.len() != 16 {
        return Err(WyrdError::Validation {
            message: "trace_id must contain exactly 16 bytes".to_owned(),
            details: serde_json::json!({ "trace_id": trace_id, "bytes": bytes.len() }),
        });
    }
    Ok(bytes)
}

/// Build the `LogicalPlan` for `QueryTraces` (filtered spans; aggregation deferred to handler).
pub async fn build_query_traces_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryTracesRequest,
) -> Result<LogicalPlan, WyrdError> {
    let df = typed_dataframe(
        state,
        caller,
        "vala.query.typed.query_traces",
        "vala.traces.spans",
    )
    .await?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    // service_name is the physical column; matches req.service filter name
    let df = opt_filter_str(df, "service_name", &req.service)?;
    let df = opt_filter_status(df, &req.status)?;
    let df = opt_filter_str(df, "name", &req.name)?;
    let df = opt_filter_min_duration(df, req.min_duration_ms)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryRecentTraces` (ordered recent spans;
/// handler groups into TraceSummaryRow).
pub async fn build_query_recent_traces_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryRecentTracesRequest,
) -> Result<LogicalPlan, WyrdError> {
    let df = typed_dataframe(
        state,
        caller,
        "vala.query.typed.query_recent_traces",
        "vala.traces.spans",
    )
    .await?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_str(df, "service_name", &req.service)?;
    let df = opt_filter_status(df, &req.status)?;
    let df = opt_filter_min_duration(df, req.min_duration_ms)?;

    Ok(df.logical_plan().clone())
}

/// Canonical `gen_ai.operation.name` values that denote a generation span.
///
/// Embedding, retrieval, agent, workflow, planning, memory, fetch-response, and
/// tool spans carry other operation names and are excluded by membership rather
/// than by scanning the attributes payload.
const GENAI_GENERATION_OPERATIONS: [&str; 3] = ["chat", "generate_content", "text_completion"];

/// Metadata and promoted columns the GenAI generation search always projects.
const GENAI_BASE_COLUMNS: [&str; 6] = [
    "start_time_unix_nano",
    "gen_ai_conversation_id",
    "gen_ai_request_model",
    "gen_ai_provider_name",
    "gen_ai_usage_input_tokens",
    "gen_ai_usage_output_tokens",
];

/// Build the `LogicalPlan` for `QueryGenAi` over canonical spans.
///
/// Generation membership and every request predicate bind to the promoted
/// `gen_ai_*` columns, so classification never decodes the attributes payload.
/// The base projection is metadata and promotions only; the canonical
/// `attributes` column — the single source of the structured input/output
/// messages — is added to the projection only for a caller holding
/// `BifrostGenAiPayload:Read`, so an unauthorized plan never scans it.
///
/// # Errors
///
/// Returns the stable authorization or audit error from [`typed_dataframe`], or
/// a plan-construction error.
pub async fn build_query_genai_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryGenAiRequest,
) -> Result<LogicalPlan, WyrdError> {
    let df = typed_dataframe(
        state,
        caller,
        "vala.query.typed.query_genai",
        "vala.traces.spans",
    )
    .await?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = df
        .filter(
            col("gen_ai_operation_name").in_list(
                GENAI_GENERATION_OPERATIONS
                    .iter()
                    .map(|op| lit(*op))
                    .collect(),
                false,
            ),
        )
        .map_err(df_err)?;
    let df = opt_filter_str(df, "gen_ai_conversation_id", &req.conversation_id)?;
    let df = opt_filter_str(df, "gen_ai_request_model", &req.model)?;
    let df = opt_filter_str(df, "gen_ai_provider_name", &req.provider)?;

    let mut projection: Vec<_> = GENAI_BASE_COLUMNS.iter().map(|name| col(*name)).collect();
    if has_perm(caller, Resource::BifrostGenAiPayload) {
        projection.push(col("attributes"));
    }
    let df = df.select(projection).map_err(df_err)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryEval` (uses `vala.eval.assertions` for per-metric rows).
pub async fn build_query_eval_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryEvalRequest,
) -> Result<LogicalPlan, WyrdError> {
    let df = typed_dataframe(
        state,
        caller,
        "vala.query.typed.query_eval",
        "vala.eval.assertions",
    )
    .await?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    // eval_ref maps to EvalRow.eval_id in the extraction layer
    let df = opt_filter_str(df, "eval_ref", &req.eval_id)?;
    // run_id is a correlation column appended by the observation policy
    let df = opt_filter_str(df, "run_id", &req.run_id)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryDrift`.
pub async fn build_query_drift_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryDriftRequest,
) -> Result<LogicalPlan, WyrdError> {
    let df = typed_dataframe(
        state,
        caller,
        "vala.query.typed.query_drift",
        "vala.drift.observations",
    )
    .await?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    // series maps to DriftRow.feature in the extraction layer
    let df = opt_filter_str(df, "series", &req.feature)?;
    // run_id is the correlation column
    let df = opt_filter_str(df, "run_id", &req.run_id)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryMetrics`.
pub async fn build_query_metrics_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryMetricsRequest,
) -> Result<LogicalPlan, WyrdError> {
    let df = typed_dataframe(
        state,
        caller,
        "vala.query.typed.query_metrics",
        "vala.metrics.points",
    )
    .await?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_str(df, "metric_name", &req.metric_name)?;
    let df = opt_filter_str(df, "metric_type", &req.metric_type)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryLogs`. Payload columns (`body`, `attributes`)
/// are projected away unless the caller holds `BifrostLogPayload:Read`.
pub async fn build_query_logs_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryLogsRequest,
) -> Result<LogicalPlan, WyrdError> {
    let df = typed_dataframe(
        state,
        caller,
        "vala.query.typed.query_logs",
        "vala.logs.records",
    )
    .await?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_i32(df, "severity_number", req.severity_number_min)?;
    // trace_id in logs is FixedSizeBinary(16); string filter deferred to Stage 5
    // event_name is Utf8 — safe to filter directly
    let df = opt_filter_str(df, "event_name", &req.event_name)?;
    let df = if !has_perm(caller, Resource::BifrostLogPayload) {
        df.drop_columns(&["body", "attributes"]).map_err(df_err)?
    } else {
        df
    };

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryAgentTraces`. Payload column (`messages`)
/// is projected away unless the caller holds `BifrostAgentTracePayload:Read`.
pub async fn build_query_agent_traces_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryAgentTracesRequest,
) -> Result<LogicalPlan, WyrdError> {
    let df = typed_dataframe(
        state,
        caller,
        "vala.query.typed.query_agent_traces",
        "vala.dev.agent_traces",
    )
    .await?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_str(df, "dev_session_id", &req.dev_session_id)?;
    let df = opt_filter_str(df, "repo", &req.repo)?;
    let df = opt_filter_str(df, "commit_sha", &req.commit_sha)?;
    let df = opt_filter_str(df, "branch", &req.branch)?;
    let df = opt_filter_str(df, "run_id", &req.run_id)?;
    let df = if !has_perm(caller, Resource::BifrostAgentTracePayload) {
        df.drop_columns(&["messages", "tool_io"]).map_err(df_err)?
    } else {
        df
    };

    Ok(df.logical_plan().clone())
}
