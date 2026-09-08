//! `wyrd.v1.ValaQueryService` gRPC implementation.
//!
//! Auth is completed in each handler (same pattern as the Gate ingest service):
//! `x-wyrd-access-token` bearer is extracted from the tonic `MetadataMap`,
//! verified via `state.auth.token_verifier`, and wrapped into a `Caller`.
//! Plan building and execution reuse the same `vala_query::service` and
//! `query::service::run_typed_query` seam the HTTP routes use.

use chrono::DateTime;
use wyrd_spec::vala::api::QueryWindow;
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::vala_query_service_server::{ValaQueryService, ValaQueryServiceServer};
use wyrd_tonic::wyrd::v1::{
    self as proto, GetTraceRequest as PGetTraceRequest, GetTraceResponse as PGetTraceResponse,
    QueryAgentTracesRequest as PQueryAgentTracesRequest,
    QueryAgentTracesResponse as PQueryAgentTracesResponse, QueryDriftRequest as PQueryDriftRequest,
    QueryDriftResponse as PQueryDriftResponse, QueryEvalRequest as PQueryEvalRequest,
    QueryEvalResponse as PQueryEvalResponse, QueryGenAiRequest as PQueryGenAiRequest,
    QueryGenAiResponse as PQueryGenAiResponse, QueryLogsRequest as PQueryLogsRequest,
    QueryLogsResponse as PQueryLogsResponse, QueryMetricsRequest as PQueryMetricsRequest,
    QueryMetricsResponse as PQueryMetricsResponse, QueryRecentTracesRequest as PQueryRecentRequest,
    QueryRecentTracesResponse as PQueryRecentResponse, QueryTracesRequest as PQueryTracesRequest,
    QueryTracesResponse as PQueryTracesResponse,
};

use crate::AppState;
use crate::components::auth::Caller;
use crate::query::service::run_typed_query;
use crate::vala_query::page_token;
use crate::vala_query::routes::{
    aggregate_spans_to_summaries, extract_agent_trace_rows, extract_drift_rows, extract_eval_rows,
    extract_genai_rows, extract_log_rows, extract_metric_rows, extract_span_rows_filtered,
};
use crate::vala_query::service::effective_limit;

/// gRPC service implementation for `wyrd.v1.ValaQueryService`.
pub struct ValaQueryGrpc {
    state: AppState,
}

impl ValaQueryGrpc {
    #[must_use]
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    #[must_use]
    pub fn into_server(self) -> ValaQueryServiceServer<Self> {
        ValaQueryServiceServer::new(self)
    }
}

// ─── auth helper ─────────────────────────────────────────────────────────────

async fn caller_from_metadata(
    state: &AppState,
    metadata: &wyrd_tonic::tonic::metadata::MetadataMap,
) -> Result<Caller, Status> {
    let verifier = state
        .auth
        .token_verifier
        .clone()
        .ok_or_else(|| Status::unavailable("auth backend not configured"))?;
    let auth = vala_bifrost_redux::gate::auth::authenticate(verifier.as_ref(), metadata)
        .await
        .map_err(|e| Status::unauthenticated(e.to_string()))?;
    let request_id = vala_bifrost_redux::gate::auth::read_or_mint_request_id(metadata);
    Ok(Caller {
        data_tenant_id: auth.tenant,
        principal: auth.principal,
        request_id,
        delegation_chain: auth.delegation_chain,
    })
}

// ─── proto→api converters ─────────────────────────────────────────────────────

/// Convert one protobuf query window into its API form.
///
/// # Errors
///
/// Returns `invalid_argument` when either bound is non-empty and not RFC3339,
/// propagated from [`proto_timestamp`].
fn proto_window(w: proto::QueryWindow) -> Result<QueryWindow, Status> {
    Ok(QueryWindow {
        since: proto_timestamp(&w.since)?,
        until: proto_timestamp(&w.until)?,
        limit: if w.limit == 0 { None } else { Some(w.limit) },
        page_token: if w.page_token.is_empty() {
            None
        } else {
            Some(w.page_token)
        },
    })
}

/// Parse one RFC3339 protobuf timestamp field; an empty string means unbounded.
///
/// This is the sole parser for every protobuf bound, so malformed text is
/// refused before planning instead of silently collapsing into an unbounded
/// scan that would return unrelated rows and disagree with the HTTP surface.
///
/// # Errors
///
/// Returns `invalid_argument` when `value` is non-empty and not RFC3339.
fn proto_timestamp(value: &str) -> Result<Option<DateTime<chrono::Utc>>, Status> {
    if value.is_empty() {
        return Ok(None);
    }
    DateTime::parse_from_rfc3339(value)
        .map(|dt| Some(dt.with_timezone(&chrono::Utc)))
        .map_err(|error| {
            Status::invalid_argument(format!("`{value}` is not an RFC3339 timestamp: {error}"))
        })
}

fn opt_str(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

fn opt_i32(v: i32) -> Option<i32> {
    if v == 0 { None } else { Some(v) }
}

fn opt_u32(v: u32) -> Option<u32> {
    if v == 0 { None } else { Some(v) }
}

// ─── api→proto row converters ─────────────────────────────────────────────────

/// Serialize one payload-gated JSON value for the protobuf projection.
///
/// `None` — the shape an unauthorized projection produces — stays absent on the
/// wire rather than becoming an empty string, so a gRPC caller sees the same
/// omission an HTTP caller sees.
fn opt_json(value: Option<serde_json::Value>) -> Option<String> {
    value.map(|value| value.to_string())
}

fn span_event_to_proto(r: wyrd_spec::vala::api::SpanEventRow) -> proto::SpanEventRow {
    proto::SpanEventRow {
        time_unix_nano: r.time_unix_nano,
        name: r.name,
        attributes_json: opt_json(r.attributes),
        dropped_attributes_count: r.dropped_attributes_count,
    }
}

fn span_link_to_proto(r: wyrd_spec::vala::api::SpanLinkRow) -> proto::SpanLinkRow {
    proto::SpanLinkRow {
        linked_trace_id: r.linked_trace_id,
        linked_span_id: r.linked_span_id,
        trace_state: r.trace_state,
        flags: r.flags,
        attributes_json: opt_json(r.attributes),
        dropped_attributes_count: r.dropped_attributes_count,
    }
}

fn span_row_to_proto(r: wyrd_spec::vala::api::SpanRow) -> proto::SpanRow {
    proto::SpanRow {
        span_id: r.span_id,
        parent_span_id: r.parent_span_id,
        trace_state: r.trace_state,
        flags: r.flags,
        name: r.name,
        kind: r.kind,
        start_time_unix_nano: r.start_time_unix_nano,
        end_time_unix_nano: r.end_time_unix_nano,
        duration_nano: r.duration_nano,
        status_code: r.status_code,
        status_message: r.status_message,
        attributes_json: opt_json(r.attributes),
        dropped_attributes_count: r.dropped_attributes_count,
        events: r.events.map(|events| proto::SpanEvents {
            events: events.into_iter().map(span_event_to_proto).collect(),
        }),
        dropped_events_count: r.dropped_events_count,
        links: r.links.map(|links| proto::SpanLinks {
            links: links.into_iter().map(span_link_to_proto).collect(),
        }),
        dropped_links_count: r.dropped_links_count,
        service_name: r.service_name,
        resource_attributes_json: opt_json(r.resource_attributes),
        resource_dropped_attributes_count: r.resource_dropped_attributes_count,
        resource_schema_url: r.resource_schema_url,
        scope_name: r.scope_name,
        scope_version: r.scope_version,
        scope_attributes_json: opt_json(r.scope_attributes),
        scope_dropped_attributes_count: r.scope_dropped_attributes_count,
        scope_schema_url: r.scope_schema_url,
    }
}

fn trace_summary_to_proto(r: wyrd_spec::vala::api::TraceSummaryRow) -> proto::TraceSummaryRow {
    proto::TraceSummaryRow {
        trace_id: r.trace_id,
        root_name: r.root_name,
        service: r.service,
        started_at: r.started_at.to_rfc3339(),
        duration_ms: r.duration_ms,
        span_count: r.span_count,
        error: r.error,
    }
}

fn genai_row_to_proto(r: wyrd_spec::vala::api::GenAiRow) -> proto::GenAiRow {
    proto::GenAiRow {
        conversation_id: r.conversation_id,
        model: r.model,
        provider: r.provider,
        start_time_unix_nano: r.start_time_unix_nano,
        input_tokens: r.input_tokens,
        output_tokens: r.output_tokens,
        input_messages_json: opt_json(r.input_messages),
        output_messages_json: opt_json(r.output_messages),
    }
}

fn eval_row_to_proto(r: wyrd_spec::vala::api::EvalRow) -> proto::EvalRow {
    proto::EvalRow {
        eval_id: r.eval_id,
        run_id: r.run_id,
        metric: r.metric,
        score: r.score,
        started_at: r.started_at.to_rfc3339(),
    }
}

fn drift_row_to_proto(r: wyrd_spec::vala::api::DriftRow) -> proto::DriftRow {
    proto::DriftRow {
        feature: r.feature,
        run_id: r.run_id.unwrap_or_default(),
        drift_score: r.drift_score,
        threshold: r.threshold.unwrap_or(0.0),
        computed_at: r.computed_at.to_rfc3339(),
    }
}

fn metric_row_to_proto(r: wyrd_spec::vala::api::MetricRow) -> proto::MetricRow {
    proto::MetricRow {
        metric_name: r.metric_name,
        metric_type: r.metric_type,
        value: r.value,
        timestamp: r.timestamp.to_rfc3339(),
        attributes_json: r.attributes.map(|v| v.to_string()).unwrap_or_default(),
    }
}

fn log_row_to_proto(r: wyrd_spec::vala::api::LogRow) -> proto::LogRow {
    proto::LogRow {
        timestamp: r.timestamp.to_rfc3339(),
        severity_number: r.severity_number,
        severity_text: r.severity_text,
        trace_id: r.trace_id.unwrap_or_default(),
        span_id: r.span_id.unwrap_or_default(),
        event_name: r.event_name.unwrap_or_default(),
        body_json: opt_json(r.body),
    }
}

fn agent_trace_to_proto(r: wyrd_spec::vala::api::AgentTraceRow) -> proto::AgentTraceRow {
    proto::AgentTraceRow {
        dev_session_id: r.dev_session_id,
        repo: r.repo,
        commit_sha: r.commit_sha.unwrap_or_default(),
        branch: r.branch.unwrap_or_default(),
        run_id: r.run_id.unwrap_or_default(),
        started_at: r.started_at.to_rfc3339(),
        payload_json: r.payload.map(|v| v.to_string()).unwrap_or_default(),
    }
}

// ─── page-token helper ────────────────────────────────────────────────────────

fn grpc_next_page_token(
    has_more: bool,
    state: &AppState,
    caller: &Caller,
    route: &str,
    qhash: u64,
) -> String {
    if !has_more {
        return String::new();
    }
    let Some(key) = super::sealing_key(state) else {
        return String::new();
    };
    let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
    let body = page_token::PageTokenBody::new(
        caller.data_tenant_id.to_string(),
        auth_hash,
        route,
        qhash,
        String::new(),
        None,
        page_token::DEFAULT_PAGE_TOKEN_TTL_SECS,
    );
    page_token::encode(&body, key).unwrap_or_default()
}

// ─── status error helper ──────────────────────────────────────────────────────

fn status_from_wyrd(e: wyrd_spec::error::WyrdError) -> Status {
    let http_code = e.status();
    let code = if http_code == 403 {
        wyrd_tonic::tonic::Code::PermissionDenied
    } else if http_code == 404 {
        wyrd_tonic::tonic::Code::NotFound
    } else if http_code == 400 {
        wyrd_tonic::tonic::Code::InvalidArgument
    } else {
        wyrd_tonic::tonic::Code::Internal
    };
    Status::new(code, e.to_string())
}

// ─── service impl ─────────────────────────────────────────────────────────────

#[wyrd_tonic::tonic::async_trait]
impl ValaQueryService for ValaQueryGrpc {
    async fn get_trace(
        &self,
        request: Request<PGetTraceRequest>,
    ) -> Result<Response<PGetTraceResponse>, Status> {
        use crate::vala_query::service::build_get_trace_plan;

        let caller = caller_from_metadata(&self.state, request.metadata()).await?;
        let p = request.into_inner();
        let api_req = wyrd_spec::vala::api::GetTraceRequest {
            trace_id: p.trace_id,
            since: proto_timestamp(&p.since)?,
            until: proto_timestamp(&p.until)?,
        };
        let plan = build_get_trace_plan(&self.state, &caller, &api_req)
            .await
            .map_err(status_from_wyrd)?;
        let (batches, _) = run_typed_query(&self.state, &caller, plan, None)
            .await
            .map_err(status_from_wyrd)?;

        let spans: Vec<proto::SpanRow> = extract_span_rows_filtered(&batches, &api_req.trace_id)
            .map_err(status_from_wyrd)?
            .into_iter()
            .map(span_row_to_proto)
            .collect();
        let waterfall = proto::TraceWaterfall {
            trace_id: api_req.trace_id,
            spans,
        };
        Ok(Response::new(PGetTraceResponse {
            trace: Some(waterfall),
        }))
    }

    async fn query_traces(
        &self,
        request: Request<PQueryTracesRequest>,
    ) -> Result<Response<PQueryTracesResponse>, Status> {
        use crate::vala_query::service::build_query_traces_plan;

        let caller = caller_from_metadata(&self.state, request.metadata()).await?;
        let p = request.into_inner();
        let api_req = wyrd_spec::vala::api::QueryTracesRequest {
            window: proto_window(p.window.unwrap_or_default())?,
            service: opt_str(p.service),
            min_duration_ms: opt_u32(p.min_duration_ms),
            status: opt_str(p.status),
            name: opt_str(p.name),
        };
        let limit = effective_limit(api_req.window.limit);
        let qhash = page_token::query_hash(&[
            api_req.service.as_deref().unwrap_or(""),
            api_req.status.as_deref().unwrap_or(""),
            api_req.name.as_deref().unwrap_or(""),
        ]);
        if let Some(token) = &api_req.window.page_token
            && let Some(key) = super::sealing_key(&self.state)
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
            .map_err(|e| status_from_wyrd(e.into()))?;
        }
        let plan = build_query_traces_plan(&self.state, &caller, &api_req)
            .await
            .map_err(status_from_wyrd)?;
        let (batches, has_more) = run_typed_query(&self.state, &caller, plan, Some(limit))
            .await
            .map_err(status_from_wyrd)?;

        let rows: Vec<proto::TraceSummaryRow> = aggregate_spans_to_summaries(&batches)
            .into_iter()
            .map(trace_summary_to_proto)
            .collect();
        let next_page_token =
            grpc_next_page_token(has_more, &self.state, &caller, "query_traces", qhash);
        Ok(Response::new(PQueryTracesResponse {
            rows,
            next_page_token,
        }))
    }

    async fn query_recent_traces(
        &self,
        request: Request<PQueryRecentRequest>,
    ) -> Result<Response<PQueryRecentResponse>, Status> {
        use crate::vala_query::service::build_query_recent_traces_plan;

        let caller = caller_from_metadata(&self.state, request.metadata()).await?;
        let p = request.into_inner();
        let api_req = wyrd_spec::vala::api::QueryRecentTracesRequest {
            window: proto_window(p.window.unwrap_or_default())?,
            service: opt_str(p.service),
            status: opt_str(p.status),
            min_duration_ms: opt_u32(p.min_duration_ms),
        };
        let limit = effective_limit(api_req.window.limit);
        let qhash = page_token::query_hash(&[
            api_req.service.as_deref().unwrap_or(""),
            api_req.status.as_deref().unwrap_or(""),
        ]);
        if let Some(token) = &api_req.window.page_token
            && let Some(key) = super::sealing_key(&self.state)
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
            .map_err(|e| status_from_wyrd(e.into()))?;
        }
        let plan = build_query_recent_traces_plan(&self.state, &caller, &api_req)
            .await
            .map_err(status_from_wyrd)?;
        let (batches, has_more) = run_typed_query(&self.state, &caller, plan, Some(limit))
            .await
            .map_err(status_from_wyrd)?;

        let rows: Vec<proto::TraceSummaryRow> = aggregate_spans_to_summaries(&batches)
            .into_iter()
            .map(trace_summary_to_proto)
            .collect();

        let next_page_token =
            grpc_next_page_token(has_more, &self.state, &caller, "query_recent_traces", qhash);
        Ok(Response::new(PQueryRecentResponse {
            rows,
            next_page_token,
        }))
    }

    async fn query_gen_ai(
        &self,
        request: Request<PQueryGenAiRequest>,
    ) -> Result<Response<PQueryGenAiResponse>, Status> {
        use crate::vala_query::service::build_query_genai_plan;

        let caller = caller_from_metadata(&self.state, request.metadata()).await?;
        let p = request.into_inner();
        let api_req = wyrd_spec::vala::api::QueryGenAiRequest {
            window: proto_window(p.window.unwrap_or_default())?,
            conversation_id: opt_str(p.conversation_id),
            model: opt_str(p.model),
            provider: opt_str(p.provider),
        };
        let limit = effective_limit(api_req.window.limit);
        let qhash = page_token::query_hash(&[
            api_req.conversation_id.as_deref().unwrap_or(""),
            api_req.model.as_deref().unwrap_or(""),
            api_req.provider.as_deref().unwrap_or(""),
        ]);
        if let Some(token) = &api_req.window.page_token
            && let Some(key) = super::sealing_key(&self.state)
        {
            let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
            let tenant_id_str = caller.data_tenant_id.to_string();
            page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_genai", qhash)
                .map_err(|e| status_from_wyrd(e.into()))?;
        }
        let plan = build_query_genai_plan(&self.state, &caller, &api_req)
            .await
            .map_err(status_from_wyrd)?;
        let (batches, has_more) = run_typed_query(&self.state, &caller, plan, Some(limit))
            .await
            .map_err(status_from_wyrd)?;

        let rows: Vec<proto::GenAiRow> = extract_genai_rows(&batches)
            .map_err(status_from_wyrd)?
            .into_iter()
            .map(genai_row_to_proto)
            .collect();
        let next_page_token =
            grpc_next_page_token(has_more, &self.state, &caller, "query_genai", qhash);
        Ok(Response::new(PQueryGenAiResponse {
            rows,
            next_page_token,
        }))
    }

    async fn query_eval(
        &self,
        request: Request<PQueryEvalRequest>,
    ) -> Result<Response<PQueryEvalResponse>, Status> {
        use crate::vala_query::service::build_query_eval_plan;

        let caller = caller_from_metadata(&self.state, request.metadata()).await?;
        let p = request.into_inner();
        let api_req = wyrd_spec::vala::api::QueryEvalRequest {
            window: proto_window(p.window.unwrap_or_default())?,
            eval_id: opt_str(p.eval_id),
            run_id: opt_str(p.run_id),
        };
        let limit = effective_limit(api_req.window.limit);
        let qhash = page_token::query_hash(&[
            api_req.eval_id.as_deref().unwrap_or(""),
            api_req.run_id.as_deref().unwrap_or(""),
        ]);
        if let Some(token) = &api_req.window.page_token
            && let Some(key) = super::sealing_key(&self.state)
        {
            let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
            let tenant_id_str = caller.data_tenant_id.to_string();
            page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_eval", qhash)
                .map_err(|e| status_from_wyrd(e.into()))?;
        }
        let plan = build_query_eval_plan(&self.state, &caller, &api_req)
            .await
            .map_err(status_from_wyrd)?;
        let (batches, has_more) = run_typed_query(&self.state, &caller, plan, Some(limit))
            .await
            .map_err(status_from_wyrd)?;

        let rows: Vec<proto::EvalRow> = extract_eval_rows(&batches)
            .into_iter()
            .map(eval_row_to_proto)
            .collect();
        let next_page_token =
            grpc_next_page_token(has_more, &self.state, &caller, "query_eval", qhash);
        Ok(Response::new(PQueryEvalResponse {
            rows,
            next_page_token,
        }))
    }

    async fn query_drift(
        &self,
        request: Request<PQueryDriftRequest>,
    ) -> Result<Response<PQueryDriftResponse>, Status> {
        use crate::vala_query::service::build_query_drift_plan;

        let caller = caller_from_metadata(&self.state, request.metadata()).await?;
        let p = request.into_inner();
        let api_req = wyrd_spec::vala::api::QueryDriftRequest {
            window: proto_window(p.window.unwrap_or_default())?,
            feature: opt_str(p.feature),
            run_id: opt_str(p.run_id),
        };
        let limit = effective_limit(api_req.window.limit);
        let qhash = page_token::query_hash(&[
            api_req.feature.as_deref().unwrap_or(""),
            api_req.run_id.as_deref().unwrap_or(""),
        ]);
        if let Some(token) = &api_req.window.page_token
            && let Some(key) = super::sealing_key(&self.state)
        {
            let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
            let tenant_id_str = caller.data_tenant_id.to_string();
            page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_drift", qhash)
                .map_err(|e| status_from_wyrd(e.into()))?;
        }
        let plan = build_query_drift_plan(&self.state, &caller, &api_req)
            .await
            .map_err(status_from_wyrd)?;
        let (batches, has_more) = run_typed_query(&self.state, &caller, plan, Some(limit))
            .await
            .map_err(status_from_wyrd)?;

        let rows: Vec<proto::DriftRow> = extract_drift_rows(&batches)
            .into_iter()
            .map(drift_row_to_proto)
            .collect();
        let next_page_token =
            grpc_next_page_token(has_more, &self.state, &caller, "query_drift", qhash);
        Ok(Response::new(PQueryDriftResponse {
            rows,
            next_page_token,
        }))
    }

    async fn query_metrics(
        &self,
        request: Request<PQueryMetricsRequest>,
    ) -> Result<Response<PQueryMetricsResponse>, Status> {
        use crate::vala_query::service::build_query_metrics_plan;

        let caller = caller_from_metadata(&self.state, request.metadata()).await?;
        let p = request.into_inner();
        let api_req = wyrd_spec::vala::api::QueryMetricsRequest {
            window: proto_window(p.window.unwrap_or_default())?,
            metric_name: opt_str(p.metric_name),
            metric_type: opt_str(p.metric_type),
        };
        let limit = effective_limit(api_req.window.limit);
        let qhash = page_token::query_hash(&[
            api_req.metric_name.as_deref().unwrap_or(""),
            api_req.metric_type.as_deref().unwrap_or(""),
        ]);
        if let Some(token) = &api_req.window.page_token
            && let Some(key) = super::sealing_key(&self.state)
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
            .map_err(|e| status_from_wyrd(e.into()))?;
        }
        let plan = build_query_metrics_plan(&self.state, &caller, &api_req)
            .await
            .map_err(status_from_wyrd)?;
        let (batches, has_more) = run_typed_query(&self.state, &caller, plan, Some(limit))
            .await
            .map_err(status_from_wyrd)?;

        let rows: Vec<proto::MetricRow> = extract_metric_rows(&batches)
            .into_iter()
            .map(metric_row_to_proto)
            .collect();
        let next_page_token =
            grpc_next_page_token(has_more, &self.state, &caller, "query_metrics", qhash);
        Ok(Response::new(PQueryMetricsResponse {
            rows,
            next_page_token,
        }))
    }

    async fn query_logs(
        &self,
        request: Request<PQueryLogsRequest>,
    ) -> Result<Response<PQueryLogsResponse>, Status> {
        use crate::vala_query::service::build_query_logs_plan;

        let caller = caller_from_metadata(&self.state, request.metadata()).await?;
        let p = request.into_inner();
        let api_req = wyrd_spec::vala::api::QueryLogsRequest {
            window: proto_window(p.window.unwrap_or_default())?,
            severity_number_min: opt_i32(p.severity_number_min),
            trace_id: opt_str(p.trace_id),
            event_name: opt_str(p.event_name),
        };
        let limit = effective_limit(api_req.window.limit);
        let qhash = page_token::query_hash(&[api_req.trace_id.as_deref().unwrap_or("")]);
        if let Some(token) = &api_req.window.page_token
            && let Some(key) = super::sealing_key(&self.state)
        {
            let auth_hash = page_token::permissions_hash(&caller.principal.effective_permissions);
            let tenant_id_str = caller.data_tenant_id.to_string();
            page_token::verify(token, key, &tenant_id_str, &auth_hash, "query_logs", qhash)
                .map_err(|e| status_from_wyrd(e.into()))?;
        }
        let plan = build_query_logs_plan(&self.state, &caller, &api_req)
            .await
            .map_err(status_from_wyrd)?;
        let (batches, has_more) = run_typed_query(&self.state, &caller, plan, Some(limit))
            .await
            .map_err(status_from_wyrd)?;

        let rows: Vec<proto::LogRow> = extract_log_rows(&batches)
            .map_err(status_from_wyrd)?
            .into_iter()
            .map(log_row_to_proto)
            .collect();
        let next_page_token =
            grpc_next_page_token(has_more, &self.state, &caller, "query_logs", qhash);
        Ok(Response::new(PQueryLogsResponse {
            rows,
            next_page_token,
        }))
    }

    async fn query_agent_traces(
        &self,
        request: Request<PQueryAgentTracesRequest>,
    ) -> Result<Response<PQueryAgentTracesResponse>, Status> {
        use crate::vala_query::service::build_query_agent_traces_plan;

        let caller = caller_from_metadata(&self.state, request.metadata()).await?;
        let p = request.into_inner();
        let api_req = wyrd_spec::vala::api::QueryAgentTracesRequest {
            window: proto_window(p.window.unwrap_or_default())?,
            dev_session_id: opt_str(p.dev_session_id),
            repo: opt_str(p.repo),
            commit_sha: opt_str(p.commit_sha),
            branch: opt_str(p.branch),
            run_id: opt_str(p.run_id),
        };
        let limit = effective_limit(api_req.window.limit);
        let qhash = page_token::query_hash(&[
            api_req.dev_session_id.as_deref().unwrap_or(""),
            api_req.repo.as_deref().unwrap_or(""),
            api_req.run_id.as_deref().unwrap_or(""),
        ]);
        if let Some(token) = &api_req.window.page_token
            && let Some(key) = super::sealing_key(&self.state)
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
            .map_err(|e| status_from_wyrd(e.into()))?;
        }
        let plan = build_query_agent_traces_plan(&self.state, &caller, &api_req)
            .await
            .map_err(status_from_wyrd)?;
        let (batches, has_more) = run_typed_query(&self.state, &caller, plan, Some(limit))
            .await
            .map_err(status_from_wyrd)?;

        let rows: Vec<proto::AgentTraceRow> = extract_agent_trace_rows(&batches)
            .into_iter()
            .map(agent_trace_to_proto)
            .collect();
        let next_page_token =
            grpc_next_page_token(has_more, &self.state, &caller, "query_agent_traces", qhash);
        Ok(Response::new(PQueryAgentTracesResponse {
            rows,
            next_page_token,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every protobuf bound is parsed by one fallible owner.
    ///
    /// An empty field stays unbounded, a valid field normalizes to UTC, and
    /// malformed non-empty text is refused with `invalid_argument` before any
    /// plan is built — so a typed gRPC query can never silently widen into an
    /// unbounded scan the caller did not ask for.
    #[test]
    fn protobuf_bounds_reject_malformed_text_and_keep_empty_unbounded() {
        assert_eq!(
            proto_timestamp("").expect("an empty bound is unbounded"),
            None
        );
        assert_eq!(
            proto_timestamp("2026-07-01T00:00:00Z").expect("a valid bound parses"),
            Some(
                "2026-07-01T00:00:00Z"
                    .parse()
                    .expect("a fixed bound parses")
            )
        );
        for malformed in ["not-a-time", "2026-07-01", "2026-07-01T00:00:00"] {
            let status = proto_timestamp(malformed).expect_err("malformed text is refused");
            assert_eq!(
                status.code(),
                wyrd_tonic::tonic::Code::InvalidArgument,
                "`{malformed}` is refused as an invalid argument"
            );
        }

        let status = proto_window(proto::QueryWindow {
            since: "not-a-time".to_owned(),
            ..proto::QueryWindow::default()
        })
        .expect_err("a window carrying a malformed bound is refused");
        assert_eq!(status.code(), wyrd_tonic::tonic::Code::InvalidArgument);
    }
}
