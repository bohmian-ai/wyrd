//! Axum adapters for the Bifrost catalog service functions.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use wyrd_spec::error::WyrdProblem;
use wyrd_spec::vala::api::{
    BifrostTableDescription, BifrostTableEntry, RegisterTableRequest, RegisterTableResponse,
};

use crate::bifrost::service;
use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Standalone Bifrost catalog router for the `/v1` group.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/bifrost/tables", post(register).get(list))
        .route("/bifrost/tables/{namespace}/{name}", get(describe))
}

#[utoipa::path(
    post,
    path = "/v1/bifrost/tables",
    request_body = RegisterTableRequest,
    responses(
        (status = 200, description = "Table created or matched", body = RegisterTableResponse),
        (status = 400, description = "Invalid table declaration", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Bifrost table registration permission required", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 409, description = "Schema fingerprint conflict", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Catalog unavailable", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other catalog-backed refusal", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Bifrost"
)]
/// Registers one Bifrost table for the authenticated tenant, or matches an
/// existing table with the same schema fingerprint.
///
/// # Errors
///
/// Returns structured validation, authorization, fingerprint-conflict, or
/// catalog-availability errors from the catalog service.
pub(crate) async fn register(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<RegisterTableRequest>,
) -> Result<Json<RegisterTableResponse>, WyrdErrorResponse> {
    service::register_table(&state, caller, body)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

#[utoipa::path(
    get,
    path = "/v1/bifrost/tables",
    responses(
        (status = 200, description = "Visible table entries", body = Vec<BifrostTableEntry>),
        (status = 401, description = "Authentication required", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Bifrost table read permission required", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Catalog unavailable", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other catalog-backed refusal", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Bifrost"
)]
/// Lists the schema-free table entries the authenticated tenant may name.
///
/// # Errors
///
/// Returns structured authorization or catalog-availability errors from the
/// catalog service.
pub(crate) async fn list(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<Vec<BifrostTableEntry>>, WyrdErrorResponse> {
    service::list_tables(&state, caller)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

#[utoipa::path(
    get,
    path = "/v1/bifrost/tables/{namespace}/{name}",
    params(
        ("namespace" = String, Path, description = "Table namespace"),
        ("name" = String, Path, description = "Table name")
    ),
    responses(
        (status = 200, description = "Table description", body = BifrostTableDescription),
        (status = 400, description = "Invalid table name", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Bifrost table read permission required", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 404, description = "No visible table", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Catalog unavailable", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other catalog-backed refusal", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Bifrost"
)]
/// Describes one visible table with its stored user and managed columns.
///
/// # Errors
///
/// Returns structured validation, authorization, not-found, or
/// catalog-availability errors from the catalog service.
pub(crate) async fn describe(
    State(state): State<AppState>,
    caller: Caller,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<BifrostTableDescription>, WyrdErrorResponse> {
    service::describe_table(&state, caller, namespace, name)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}
