//! Axum router namespace for Wyrd server surfaces.

use axum::Router;
use axum::error_handling::HandleErrorLayer;
use axum::extract::Request;
use axum::middleware;
use tower::ServiceBuilder;
use tower::limit::ConcurrencyLimitLayer;
use tower::load_shed::LoadShedLayer;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use wyrd_spec::error::WyrdError;

use crate::bifrost::routes::router as bifrost_router;
use crate::components::admin::admin_router;
use crate::components::auth::auth_router;
use crate::components::authz::authz_router;
use crate::components::cards::cards_router;
use crate::components::eval::eval_router;
use crate::components::health::health_router;
use crate::components::platform::{
    platform_auth_router, platform_identity_router, platform_login_router, platform_router,
};
use crate::components::principals::principals_router;
use crate::components::storage::storage_router;
use crate::http::error::WyrdErrorResponse;
use crate::http::middleware::authenticate::require_authenticated;
use crate::http::openapi::WyrdApiDoc;
use crate::http::otlp::router as otlp_router;
use crate::query::routes::router as query_router;
use crate::state::AppState;

/// Build the HTTP router with shared server state.
pub fn build_router(state: AppState) -> Router {
    // Unprotected: /healthz and /readyz skip the fallible middleware stack so
    // they can respond even when inner layers are under pressure or broken.
    let unprotected = health_router().layer(CatchPanicLayer::custom(
        crate::http::error::wyrd_panic_response,
    ));

    let auth_routes = auth_router();

    // Default-deny: authentication is a property of the whole /v1 nest, not any
    // single handler. Attaching require_authenticated to v1_group *after* its
    // .fallback means Router::layer wraps the fallback too, so unknown /v1 paths
    // are rejected with 401 before v1_not_found runs (no route-existence oracle).
    // attach_request_id remains outermost on `protected`, so the RequestId
    // extension is already present when this layer runs.
    let v1_group = Router::new()
        .merge(storage_router(&state))
        .merge(eval_router())
        .merge(authz_router())
        .merge(cards_router())
        .merge(principals_router())
        .merge(admin_router())
        .merge(bifrost_router())
        .merge(query_router())
        .merge(otlp_router())
        .fallback(v1_not_found)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_authenticated,
        ));

    // One MCP endpoint on the one public listener. It sits inside the same
    // protected edge as `/v1` — request-id, panic capture, load-shed,
    // concurrency, timeout, body limit — and carries the same default-deny
    // authentication, so `rmcp` never sees an unverified caller. The route is
    // not nested under `/v1`: the MCP protocol version, not the Wyrd API
    // version, governs this surface's compatibility.
    let mcp_route = Router::new()
        .route_service("/mcp", crate::mcp::mcp_service(&state))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_authenticated,
        ));

    let protected = apply_protected_edge(
        Router::new()
            .merge(auth_routes)
            .merge(platform_auth_router())
            .merge(platform_login_router())
            .merge(platform_router())
            .merge(platform_identity_router())
            .merge(mcp_route)
            .nest("/v1", v1_group),
        &state,
    );

    Router::new()
        .merge(unprotected)
        .route(
            "/openapi.json",
            axum::routing::get(|| async { axum::Json(WyrdApiDoc::openapi()) }),
        )
        .route(
            "/openapi.yaml",
            axum::routing::get(|| async {
                (
                    [(axum::http::header::CONTENT_TYPE, "application/yaml")],
                    serde_yaml::to_string(&WyrdApiDoc::openapi())
                        .expect("OpenAPI document serializes"),
                )
            }),
        )
        .merge(protected)
        .with_state(state)
        .layer(axum::middleware::from_fn(
            crate::http::middleware::metrics::track_metrics,
        ))
        .layer(TraceLayer::new_for_http())
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
///   4. LoadShed — sheds requests when ConcurrencyLimit is not ready
///   5. ConcurrencyLimitLayer — caps in-flight requests
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
        .layer(LoadShedLayer::new())
        .layer(ConcurrencyLimitLayer::new(state.limits.concurrency))
        .layer(crate::http::middleware::edge_timeout::EdgeTimeoutLayer::new(state.limits.timeout))
        .layer(crate::http::middleware::body_limit::wyrd_body_limit(
            state.limits.body_bytes,
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

async fn v1_not_found(request: Request) -> Result<(), WyrdErrorResponse> {
    Err(WyrdError::NotFound {
        message: "Wyrd route not found".to_owned(),
        details: serde_json::json!({ "path": request.uri().path() }),
    }
    .into())
}
