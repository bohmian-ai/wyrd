//! Typed LogicalPlan builders for each ValaQueryService method (M-07).
//!
//! Each function:
//!   1. Resolves the domain table provider(s) from the bifrost catalog.
//!   2. Registers them in a temporary session context to get a schema-aware DataFrame.
//!   3. Applies typed filters as bound `Expr` literals — never string interpolation.
//!   4. Projects away sensitive payload columns when the caller lacks the payload permission.
//!   5. Returns the `LogicalPlan` (pre-analysis). The `TenantPredicateRule` analyzer
//!      injects the tenant predicate during `run_plan_query`'s `execute_logical_plan` call.
//!
//! Note on trace-summary methods (QueryTraces, QueryRecentTraces): these return spans
//! plans filtered but NOT aggregated. The route handler performs the in-Rust GROUP-BY
//! aggregation after collecting batches. Stage 5 will push aggregation into the plan.
//!
//! Physical→API schema mapping:
//!   - Binary ID columns (trace_id FixedBin16, span_id FixedBin8) are NOT filtered
//!     by string predicates in Stage 4 — binary comparison requires separate handling.
//!   - String column name differences (start_time vs started_at, service_name vs service)
//!     are resolved in the extraction layer in routes.rs.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use datafusion::common::TableReference;
use datafusion::logical_expr::LogicalPlan;
use datafusion::prelude::{DataFrame, SessionContext, col, lit};
use datafusion::scalar::ScalarValue;
use vala_bifrost::BifrostNamespace;
use vala_bifrost::error::BifrostError as EngineBifrostError;
use vala_bifrost::session::wyrd_session_context;
use wyrd_runtime::{Action, Permission, Resource};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    GetTraceRequest, MAX_QUERY_PAGE_SIZE, QueryAgentTracesRequest, QueryDriftRequest,
    QueryEvalRequest, QueryGenAiRequest, QueryLogsRequest, QueryMetricsRequest,
    QueryRecentTracesRequest, QueryTracesRequest,
};
use wyrd_spec::vala::system_columns::WYRD_EVENT_TIME;

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

/// Register a domain table provider in `ctx`. A missing table (not yet materialized)
/// is silently skipped — DataFusion planning will surface the error if the table is
/// actually referenced.
async fn register(
    ctx: &SessionContext,
    state: &AppState,
    ns: BifrostNamespace,
    table_name: &str,
    tenant: wyrd_spec::ids::DataTenantId,
    fqn: &str,
) -> Result<(), WyrdError> {
    match state.bifrost.provider(ns, table_name, tenant).await {
        Ok(provider) => {
            ctx.register_table(TableReference::bare(fqn), Arc::new(provider))
                .map_err(|e| WyrdError::Internal {
                    message: "failed to register domain table".to_owned(),
                    details: serde_json::json!({ "detail": e.to_string() }),
                })?;
        }
        Err(EngineBifrostError::TableNotFound(_)) => {}
        Err(e) => {
            return Err(WyrdError::Internal {
                message: "bifrost provider error".to_owned(),
                details: serde_json::json!({ "detail": e.to_string() }),
            });
        }
    }
    Ok(())
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

fn opt_filter_u32(df: DataFrame, column: &str, value: Option<u32>) -> Result<DataFrame, WyrdError> {
    match value {
        Some(v) => df.filter(col(column).gt_eq(lit(v as i64))).map_err(df_err),
        None => Ok(df),
    }
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

/// Build the `LogicalPlan` for `GetTrace` (spans filtered to one trace_id + window).
/// Payload `attributes` column is projected away unless the caller holds
/// `BifrostTracePayload:Read`.
pub async fn build_get_trace_plan(
    state: &AppState,
    caller: &Caller,
    req: &GetTraceRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    register(
        &ctx,
        state,
        BifrostNamespace::Traces,
        "spans",
        tenant,
        "vala.traces.spans",
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.traces.spans"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    // trace_id is FixedSizeBinary(16) in the physical schema; the string filter
    // needs a binary literal — deferred to Stage 5 hex→binary conversion.
    // For now the time-window filter scopes the scan; callers rely on filtering in Rust.
    let df = if !has_perm(caller, Resource::BifrostTracePayload) {
        df.drop_columns(&["attributes"]).map_err(df_err)?
    } else {
        df
    };

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryTraces` (filtered spans; aggregation deferred to handler).
pub async fn build_query_traces_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryTracesRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    register(
        &ctx,
        state,
        BifrostNamespace::Traces,
        "spans",
        tenant,
        "vala.traces.spans",
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.traces.spans"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    // service_name is the physical column; matches req.service filter name
    let df = opt_filter_str(df, "service_name", &req.service)?;
    let df = opt_filter_str(df, "status", &req.status)?;
    let df = opt_filter_str(df, "name", &req.name)?;
    let df = opt_filter_u32(df, "duration_ms", req.min_duration_ms)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryRecentTraces` (ordered recent spans;
/// handler groups into TraceSummaryRow).
pub async fn build_query_recent_traces_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryRecentTracesRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    register(
        &ctx,
        state,
        BifrostNamespace::Traces,
        "spans",
        tenant,
        "vala.traces.spans",
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.traces.spans"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_str(df, "service_name", &req.service)?;
    let df = opt_filter_str(df, "status", &req.status)?;
    let df = opt_filter_u32(df, "duration_ms", req.min_duration_ms)?;
    // Order by start_time DESC so the extractor can pick earliest/latest per trace
    let df = df
        .sort(vec![
            datafusion::prelude::col("start_time").sort(false, true),
        ])
        .map_err(df_err)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryGenAi`.
/// Payload columns (`input_messages`, `output_messages`) are projected away unless
/// the caller holds `BifrostGenAiPayload:Read`.
pub async fn build_query_genai_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryGenAiRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    register(
        &ctx,
        state,
        BifrostNamespace::GenAi,
        "messages",
        tenant,
        "vala.genai.messages",
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.genai.messages"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_str(df, "conversation_id", &req.conversation_id)?;
    let df = opt_filter_str(df, "request_model", &req.model)?;
    let df = opt_filter_str(df, "provider_name", &req.provider)?;
    let df = if !has_perm(caller, Resource::BifrostGenAiPayload) {
        df.drop_columns(&["input_messages", "output_messages"])
            .map_err(df_err)?
    } else {
        df
    };

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryEval` (uses `vala.eval.assertions` for per-metric rows).
pub async fn build_query_eval_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryEvalRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    // assertions table has assertion_name (metric), score_value (score), eval_ref (eval_id)
    register(
        &ctx,
        state,
        BifrostNamespace::Eval,
        "assertions",
        tenant,
        "vala.eval.assertions",
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.eval.assertions"))
        .await
        .map_err(df_err)?;
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
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    register(
        &ctx,
        state,
        BifrostNamespace::Drift,
        "observations",
        tenant,
        "vala.drift.observations",
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.drift.observations"))
        .await
        .map_err(df_err)?;
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
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    register(
        &ctx,
        state,
        BifrostNamespace::Metrics,
        "points",
        tenant,
        "vala.metrics.points",
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.metrics.points"))
        .await
        .map_err(df_err)?;
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
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    register(
        &ctx,
        state,
        BifrostNamespace::Logs,
        "records",
        tenant,
        "vala.logs.records",
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.logs.records"))
        .await
        .map_err(df_err)?;
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
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    register(
        &ctx,
        state,
        BifrostNamespace::Dev,
        "agent_traces",
        tenant,
        "vala.dev.agent_traces",
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.dev.agent_traces"))
        .await
        .map_err(df_err)?;
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
