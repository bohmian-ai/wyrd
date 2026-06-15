//! Axum router namespace for Wyrd server surfaces.

use axum::Router;
use axum::extract::Request;
use axum::middleware;
use axum::routing::get;
use wyrd_spec::error::WyrdError;

use crate::auth::AuthenticatedPrincipal;
use crate::error::WyrdErrorResponse;
use crate::state::AppState;

/// Build the HTTP router with shared server state.
pub fn build_router(state: AppState) -> Router {
    let v1 = Router::new().fallback(v1_not_found);

    Router::new()
        .route("/healthz", get(crate::healthz))
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
