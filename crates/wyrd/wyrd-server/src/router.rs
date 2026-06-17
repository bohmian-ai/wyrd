//! Axum router namespace for Wyrd server surfaces.

use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::middleware;
use axum::routing::get;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use wyrd_spec::error::WyrdError;

use crate::auth::AuthenticatedPrincipal;
use crate::error::WyrdErrorResponse;
use crate::state::AppState;

/// Build the HTTP router with shared server state.
pub fn build_router(state: AppState) -> Router {
    let v1 = crate::storage::routes::mount(Router::new(), &state).fallback(v1_not_found);

    let auth_governor = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(10)
            .burst_size(20)
            .finish()
            .expect("governor config is valid"),
    );

    Router::new()
        .route("/healthz", get(crate::healthz))
        .merge(
            crate::auth::routes::router()
                .layer(GovernorLayer::new(auth_governor)),
        )
        .nest("/v1", v1)
        .layer(middleware::from_fn(
            crate::middleware::request_id::attach_request_id,
        ))
        .with_state(state)
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
