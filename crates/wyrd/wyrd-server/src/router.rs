//! Axum router namespace for Wyrd server surfaces.

use axum::Router;
use axum::extract::Request;
use axum::error_handling::HandleErrorLayer;
use axum::middleware;
use axum::routing::get;
use tower::ServiceBuilder;
use tower::limit::ConcurrencyLimitLayer;
use tower::load_shed::LoadShedLayer;
use tower::timeout::TimeoutLayer;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::TraceLayer;
use wyrd_spec::error::WyrdError;

use crate::auth::AuthenticatedPrincipal;
use crate::error::WyrdErrorResponse;
use crate::state::AppState;

/// Build the HTTP router with shared server state.
pub fn build_router(state: AppState) -> Router {
    // Unprotected: /healthz and /readyz skip the fallible middleware stack so
    // they can respond even when inner layers are under pressure or broken.
    let unprotected = Router::new()
        .route("/healthz", get(crate::healthz))
        .route("/readyz", get(crate::health::readyz))
        .layer(CatchPanicLayer::custom(crate::error::wyrd_panic_response));

    let auth_routes = crate::auth::routes::router();

    let v1_group = crate::routes::authz::routes::mount(
        crate::storage::routes::mount(Router::new(), &state),
        &state,
    )
    .fallback(v1_not_found);

    // ServiceBuilder builds the inner error-handling middleware stack as a
    // single layer. Each layer in the builder wraps the one below it; the
    // first entry here is the outermost (runs first on each request).
    //
    // HandleErrorLayer must be outermost in the ServiceBuilder so the combined
    // service exports Error = Infallible, satisfying axum 0.8's Router::layer
    // constraint (`<L::Service as Service<Request>>::Error: Into<Infallible>`).
    //
    // Full request traversal (outermost → innermost):
    //   1. attach_request_id — mints/propagates ID; injects instance into errors
    //   2. CatchPanic — converts panics to 500 before they escape the stack
    //   3. HandleErrorLayer — maps BoxError (Elapsed, Overloaded) → HTTP response
    //   4. LoadShed — sheds requests when ConcurrencyLimit is not ready
    //   5. ConcurrencyLimitLayer — caps in-flight requests
    //   6. TimeoutLayer — enforces per-request deadline
    //   7. WyrdBodyLimit — enforces max body size
    //   8. handler
    let inner_stack = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(crate::error::map_tower_error_to_wyrd))
        .layer(LoadShedLayer::new())
        .layer(ConcurrencyLimitLayer::new(state.limits.concurrency))
        .layer(TimeoutLayer::new(state.limits.timeout))
        .layer(crate::middleware::body_limit::wyrd_body_limit(
            state.limits.body_bytes,
        ));

    let protected = Router::new()
        .merge(auth_routes)
        .nest("/v1", v1_group)
        .layer(inner_stack)
        .layer(CatchPanicLayer::custom(crate::error::wyrd_panic_response))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::request_id::attach_request_id,
        ));

    Router::new()
        .merge(unprotected)
        .merge(protected)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

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
