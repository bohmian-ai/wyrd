//! Axum router namespace for Wyrd server surfaces.

use std::sync::Arc;

use axum::Router;
use axum::error_handling::HandleErrorLayer;
use axum::extract::Request;
use axum::middleware;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::{DefaultOnResponse, OnResponse as _, TraceLayer};
use tracing::Span;
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use wyrd_spec::error::WyrdError;

use crate::bifrost::routes::router as bifrost_router;
use crate::components::admin::{admin_router, identity_router};
use crate::components::auth::AuthenticatedPrincipal;
use crate::components::auth::auth_router;
use crate::components::cards::cards_router;
use crate::components::gateway::{gateway_ingress_router, gateway_router};
use crate::components::health::health_router;
use crate::components::operators::operator_connections_router;
use crate::components::platform::{
    platform_auth_router, platform_credentials_router, platform_identity_router,
    platform_login_router, platform_router,
};
use crate::components::principals::principals_router;
use crate::components::storage::storage_router;
use crate::components::verification::verification_router;
use crate::http::error::WyrdErrorResponse;
use crate::http::middleware::authenticate::{
    require_authenticated, require_bifrost_authenticated, require_otlp_authenticated,
    require_tenant_authenticated,
};
use crate::http::openapi::{BrowserSessionAddon, ProblemMediaAddon, SecurityAddon, WyrdApiDoc};
use crate::http::otlp::router as otlp_router;
use crate::query::routes::router as query_router;
use crate::state::AppState;

/// Build the HTTP router with shared server state.
pub fn build_router(state: AppState) -> Router {
    let tenant_spans = state.bifrost.tenant_span_sink();
    // Unprotected: /healthz and /readyz skip the fallible middleware stack so
    // they can respond even when inner layers are under pressure or broken.
    let unprotected = health_router().layer(CatchPanicLayer::custom(
        crate::http::error::wyrd_panic_response,
    ));

    let auth_routes = auth_router();

    // Default-deny: authentication is a property of the whole /v1 nest, not any
    // single handler. Each route group carries the verification layer for its
    // audience: the Bifrost and query routers accept a `bifrost`-audience
    // delegated token, every other group accepts only `wyrd`, so a token bound
    // to Bifrost cannot be replayed anywhere else. Unknown /v1 paths reach
    // v1_not_found, whose `AuthenticatedPrincipal` extractor rejects with 401
    // before the 404 is built (no route-existence oracle). attach_request_id
    // remains outermost on `protected`, so the RequestId extension is already
    // present when these layers run.
    let bifrost_group = OpenApiRouter::new()
        .merge(bifrost_router())
        .merge(query_router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_bifrost_authenticated,
        ));
    // OTLP alone also admits a stock exporter's `x-wyrd-api-key`.
    let otlp_group = otlp_router().layer(middleware::from_fn_with_state(
        state.clone(),
        require_otlp_authenticated,
    ));
    let tenant_group = OpenApiRouter::new()
        .merge(storage_router(&state))
        .merge(cards_router())
        .merge(principals_router())
        .merge(verification_router(&state))
        .merge(operator_connections_router())
        .merge(admin_router())
        .merge(identity_router())
        .merge(gateway_router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_tenant_authenticated,
        ));
    // The routes whose layer also accepts a browser session, read from the
    // registrations that mount them, so the document offers the cookie there
    // and nowhere else.
    let browser_session_paths = [&tenant_group, &bifrost_group]
        .into_iter()
        .flat_map(|group| group.get_openapi().paths.paths.keys())
        .map(|path| format!("/v1{path}"))
        .collect();
    let v1_group = tenant_group
        .merge(otlp_group)
        .merge(bifrost_group)
        .fallback(v1_not_found);

    // One MCP endpoint on the one public listener. It sits inside the same
    // protected edge as `/v1` — request-id, panic capture, load-shed,
    // concurrency, timeout, body limit — and carries the same default-deny
    // authentication, so `rmcp` never sees an unverified caller. The route is
    // not nested under `/v1`: the MCP protocol version, not the Wyrd API
    // version, governs this surface's compatibility. It speaks its own
    // protocol rather than the Wyrd HTTP contract, so it is mounted on a plain
    // Axum router and is deliberately not an OpenAPI operation.
    let mcp_route = Router::new()
        .route_service("/mcp", crate::mcp::mcp_service(&state))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_authenticated,
        ));

    // Routing and documentation come out of the same composition: every handler
    // is registered once, through `routes!`, and the served document is
    // whatever that registration produced. There is no second list of paths to
    // keep in step with this one. Gateway inference sits outside `/v1`'s
    // default-deny layer because each protocol authenticates its own SDK
    // credential carrier, but it is documented by the same composition.
    let api = OpenApiRouter::with_openapi(WyrdApiDoc::openapi())
        .merge(auth_routes)
        .merge(platform_auth_router())
        .merge(platform_login_router())
        .merge(platform_router())
        .merge(platform_identity_router())
        .merge(platform_credentials_router())
        .merge(gateway_ingress_router(&state))
        .nest("/v1", v1_group);
    #[cfg(feature = "bench")]
    let api = api.merge(crate::components::platform::benchmark::platform_benchmark_router());
    let (routed, mut document) = api.split_for_parts();
    // The document-wide modifiers run after composition because they read the
    // assembled paths.
    SecurityAddon.modify(&mut document);
    BrowserSessionAddon {
        paths: browser_session_paths,
    }
    .modify(&mut document);
    ProblemMediaAddon.modify(&mut document);
    let document = Arc::new(document);

    let protected = apply_protected_edge(routed.merge(mcp_route), &state);

    Router::new()
        .merge(unprotected)
        .route(
            "/openapi.json",
            axum::routing::get(move || {
                let document = Arc::clone(&document);
                async move { axum::Json(document) }
            }),
        )
        .merge(protected)
        .with_state(state)
        .layer(axum::middleware::from_fn(
            crate::http::middleware::metrics::track_metrics,
        ))
        .layer(axum::middleware::from_fn(
            crate::http::middleware::trace_context::attach_trace_context,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(move |request: &Request| {
                    request_span(request, tenant_spans.as_ref())
                })
                .on_response(record_response),
        )
}

/// Opens the SERVER span of one HTTP request.
///
/// A valid inbound `traceparent` makes the caller's span its parent, so the
/// request and everything done on its behalf continue the caller's trace; a
/// missing, malformed, or repeated one leaves it the root of a new trace. It
/// records the method, path, and version but never the query: a URL-borne
/// credential, such as a provider SDK's `?key=` that ingress refuses, is
/// already in the request when this span opens, before any authentication
/// runs, and must not reach logs or exported traces. It also carries the
/// OpenTelemetry HTTP server attributes: `http.request.method`, `http.route`
/// as the matched route template (never the raw path), and
/// `http.response.status_code`, which [`record_response`] fills in. A
/// continued trace stages its tenant span copies on this server's
/// `tenant_spans` sink. A benchmark server attaches its declared benchmark
/// headers to the span as it opens
/// ([`wyrd_telemetry::timing::label_request`]); they select a timing
/// series only and carry no authority.
pub(crate) fn request_span(
    request: &Request,
    tenant_spans: Option<&Arc<dyn wyrd_telemetry::TenantSpanSink>>,
) -> Span {
    let span = tracing::info_span!(
        "request",
        otel.kind = "server",
        method = %request.method(),
        path = request.uri().path(),
        version = ?request.version(),
        http.request.method = %request.method(),
        http.route = request
            .extensions()
            .get::<axum::extract::MatchedPath>()
            .map(axum::extract::MatchedPath::as_str),
        http.response.status_code = tracing::field::Empty,
    );
    wyrd_telemetry::serve_request(
        &span,
        wyrd_telemetry::inbound_context(request.headers()).as_ref(),
        tenant_spans,
    );
    wyrd_telemetry::timing::label_request(&span, request.headers());
    span
}

/// Records the response status on the request span, then logs the response
/// as the default `TraceLayer` does.
fn record_response(response: &axum::response::Response, latency: std::time::Duration, span: &Span) {
    span.record("http.response.status_code", response.status().as_u16());
    DefaultOnResponse::default().on_response(response, latency, span);
}

/// Apply the core protective edge stack to `router`: request-id propagation,
/// panic→`WyrdError` capture, tower error mapping, load-shed, concurrency,
/// timeout, and body limit. This is exactly what wraps core `/v1`; it is reused
/// by `WyrdServer::merge_http_protected` so enterprise write routes get an
/// identical edge. Does NOT add authentication — callers layer
/// `require_authenticated` themselves (core does it on `v1_group`;
/// `merge_http_protected` does it before calling this).
///
/// Generic over `S` so it can be applied to both `Router<AppState>` (core) and
/// `Router<()>` (finalized enterprise routers). The protective layers derive
/// their configuration from `state: &AppState`; the router's state parameter
/// `S` is independent and unchanged.
///
/// Full request traversal (outermost → innermost):
///   1. attach_request_id — mints/propagates ID; injects instance into errors
///   2. CatchPanic — converts panics to 500 before they escape the stack
///   3. HandleErrorLayer — maps BoxError (Elapsed, Overloaded) → HTTP response
///   4. EdgeCapacity (two layers) — load-sheds requests beyond the in-flight
///      concurrency cap; `POST /v1/query` bypasses both and waits in Oracle's bounded
///      query queue instead
///   6. EdgeTimeout — enforces the per-request deadline; `POST /v1/query`
///      hands its remaining wait to the Oracle query deadline after admission
///   7. WyrdBodyLimit — enforces max body size
///   8. handler
pub(crate) fn apply_protected_edge<S>(router: Router<S>, state: &AppState) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let inner_stack = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(
            crate::http::error::map_tower_error_to_wyrd,
        ))
        .layer(
            crate::http::middleware::edge_capacity::EdgeCapacityLayer::new(
                state.settings.limits.concurrency,
            ),
        )
        .layer(
            crate::http::middleware::edge_timeout::EdgeTimeoutLayer::new(
                state.settings.limits.timeout,
            ),
        )
        .layer(crate::http::middleware::body_limit::wyrd_body_limit(
            state.settings.limits.body_bytes,
            state
                .bifrost
                .serves_api()
                .then(|| state.bifrost.gate().otlp_decoding_message_size()),
            state
                .bifrost
                .serves_api()
                .then(|| state.bifrost.transport_admission()),
        ));
    router
        .layer(inner_stack)
        .layer(CatchPanicLayer::custom(
            crate::http::error::wyrd_panic_response,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::http::middleware::request_id::attach_request_id,
        ))
}

/// Answer an unmatched `/v1` path with `404` — but only for an authenticated
/// caller.
///
/// The [`AuthenticatedPrincipal`] extractor runs first and rejects with `401`,
/// so an unauthenticated caller cannot probe which routes exist.
///
/// # Errors
/// Always returns an error: the extractor's `400`/`401`/`503`, or `404`.
async fn v1_not_found(
    _principal: AuthenticatedPrincipal,
    request: Request,
) -> Result<(), WyrdErrorResponse> {
    Err(WyrdError::NotFound {
        message: "Wyrd route not found".to_owned(),
        details: serde_json::json!({ "path": request.uri().path() }),
    }
    .into())
}
