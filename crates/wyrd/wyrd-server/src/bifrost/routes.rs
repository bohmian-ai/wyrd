//! Axum adapters for the Bifrost catalog service functions.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use wyrd_spec::vala::api::{
    BifrostTableDescription, BifrostTableEntry, RegisterTableRequest, RegisterTableResponse,
};

use crate::auth::Caller;
use crate::bifrost::service;
use crate::error::WyrdErrorResponse;
use crate::state::AppState;

/// Mount the Bifrost catalog routes into an existing `/v1` router.
pub fn mount(router: Router<AppState>, _state: &AppState) -> Router<AppState> {
    router
        .route("/bifrost/tables", post(register).get(list))
        .route("/bifrost/tables/{namespace}/{name}", get(describe))
}

async fn register(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<RegisterTableRequest>,
) -> Result<Json<RegisterTableResponse>, WyrdErrorResponse> {
    service::register_table(&state, caller, body)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

async fn list(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<Vec<BifrostTableEntry>>, WyrdErrorResponse> {
    service::list_tables(&state, caller)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

async fn describe(
    State(state): State<AppState>,
    caller: Caller,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<BifrostTableDescription>, WyrdErrorResponse> {
    service::describe_table(&state, caller, namespace, name)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}
