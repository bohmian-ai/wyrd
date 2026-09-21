//! Axum adapters for the Bifrost catalog service functions.

use axum::Json;
use axum::extract::{Path, State};
use wyrd_spec::error::WyrdProblem;
use wyrd_spec::vala::api::{
    BifrostTableDescription, BifrostTableEntry, RegisterTableRequest, RegisterTableResponse,
};

use crate::bifrost::service;
use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Standalone Bifrost catalog router for the `/v1` group.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(register, list))
        .routes(routes!(describe))
}

#[utoipa::path(
    post,
    path = "/bifrost/tables",
    request_body = RegisterTableRequest,
    responses(
        (status = 200, description = "Table created or matched", body = RegisterTableResponse),
        (status = 400, description = "The declaration is not a caller-owned dataset, or its \
          fields are not a valid Bifrost schema (WYRD_SPEC_400_VALIDATION, \
          WYRD_VALA_400_SCHEMA_PARSE, WYRD_VALA_400_BIFROST_RESERVED_COLUMN)",
         body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "The principal may not register Bifrost tables \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_VALA_403_BIFROST_RESERVED_BUILTIN_WRITE)",
         body = WyrdProblem, content_type = "application/problem+json"),
        (status = 409, description = "An existing table of this name has a different schema \
          or physical layout (WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH, \
          WYRD_VALA_409_BIFROST_COMMIT_CONFLICT)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "This server carries no catalog, the catalog or its \
          object store is unreachable, or no verifier is configured for the access \
          token (WYRD_VALA_503_SCRIBE_ROLE_UNAVAILABLE, \
          WYRD_VALA_503_BIFROST_CATALOG_UNREACHABLE, WYRD_VALA_503_BIFROST_STORAGE_UNREACHABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Any other catalog-backed refusal, each carrying \
          its own stable code from the Bifrost catalog (WYRD_VALA_500_BIFROST_INTERNAL, \
          WYRD_VALA_500_BIFROST_METADATA_MISMATCH, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
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
    path = "/bifrost/tables",
    responses(
        (status = 200, description = "Visible table entries", body = Vec<BifrostTableEntry>),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "The principal may not read the Bifrost catalog \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "This server carries no catalog, the catalog or its \
          object store is unreachable, or no verifier is configured for the access \
          token (WYRD_VALA_503_SCRIBE_ROLE_UNAVAILABLE, \
          WYRD_VALA_503_BIFROST_CATALOG_UNREACHABLE, WYRD_VALA_503_BIFROST_STORAGE_UNREACHABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Any other catalog-backed refusal, each carrying \
          its own stable code from the Bifrost catalog (WYRD_VALA_500_BIFROST_INTERNAL, \
          WYRD_VALA_500_BIFROST_METADATA_MISMATCH, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
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
    path = "/bifrost/tables/{namespace}/{name}",
    params(
        ("namespace" = String, Path, description = "Table namespace"),
        ("name" = String, Path, description = "Table name")
    ),
    responses(
        (status = 200, description = "Table description", body = BifrostTableDescription),
        (status = 400, description = "The namespace is not one Bifrost serves \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "The principal may not read the Bifrost catalog \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 404, description = "No table of that name is visible to this tenant \
          (WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "This server carries no catalog, the catalog or its \
          object store is unreachable, or no verifier is configured for the access \
          token (WYRD_VALA_503_SCRIBE_ROLE_UNAVAILABLE, \
          WYRD_VALA_503_BIFROST_CATALOG_UNREACHABLE, WYRD_VALA_503_BIFROST_STORAGE_UNREACHABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Any other catalog-backed refusal, each carrying \
          its own stable code from the Bifrost catalog (WYRD_VALA_500_BIFROST_INTERNAL, \
          WYRD_VALA_500_BIFROST_METADATA_MISMATCH, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
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
