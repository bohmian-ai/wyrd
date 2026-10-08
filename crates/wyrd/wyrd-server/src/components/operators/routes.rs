//! HTTP projection of Operator connection management.
//!
//! Five operations on `/v1/operator-connections`. Each handler parses its
//! typed path and body, then delegates to [`OperatorConnectionControl`], which
//! owns authorization, audit, sealing, and tenancy. A malformed body is
//! refused by position only, so a rejected secret value is never echoed.

use axum::Json;
use axum::body::Bytes;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::de::DeserializeOwned;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::OperatorConnectionId;
use wyrd_spec::operator_connection::{
    CreateOperatorConnectionRequest, OperatorConnectionView, UpdateOperatorConnectionRequest,
};

use super::service::OperatorConnectionControl;
use crate::components::auth::Caller;
use crate::http::error::{WyrdErrorResponse, path_rejection};
use crate::state::AppState;

/// Build the Operator connection routes for the `/v1` group.
pub fn operator_connections_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_connection, list_connections))
        .routes(routes!(
            get_connection,
            update_connection,
            disable_connection
        ))
}

/// Decode a JSON body without echoing any of its values.
///
/// # Errors
/// Returns [`WyrdError::OperatorConnectionInvalid`] naming the error class
/// and position only.
pub(crate) fn decode_body<T: DeserializeOwned>(body: &[u8]) -> Result<T, WyrdError> {
    serde_json::from_slice(body).map_err(|error| WyrdError::OperatorConnectionInvalid {
        message: format!(
            "request body is not a valid connection request ({:?} error at line {} column {})",
            error.classify(),
            error.line(),
            error.column()
        ),
        details: serde_json::json!({ "line": error.line(), "column": error.column() }),
    })
}

/// Create one Operator connection and return its redacted view.
///
/// # Errors
/// Returns a stable Wyrd error for an invalid body, missing
/// `operators:write`, an unavailable key, a name conflict, or a failed write.
#[utoipa::path(
    post,
    path = "/operator-connections",
    request_body = CreateOperatorConnectionRequest,
    responses(
        (status = 201, description = "Connection created", body = OperatorConnectionView),
        (status = 400, description = "The body is invalid \
          (WYRD_OPERATOR_400_INVALID_CONNECTION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks operators:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "A connection of this provider and name exists \
          (WYRD_OPERATOR_409_CONNECTION_CONFLICT)", body = WyrdProblem),
        (status = 503, description = "No active connection key is readable, or the \
          registry is unavailable (WYRD_OPERATOR_503_KEY_UNAVAILABLE, \
          WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Operator connections"
)]
#[tracing::instrument(
    skip(state, caller, body),
    fields(operation = "operator_connection.create")
)]
async fn create_connection(
    State(state): State<AppState>,
    caller: Caller,
    body: Bytes,
) -> Result<(StatusCode, Json<OperatorConnectionView>), WyrdErrorResponse> {
    let request: CreateOperatorConnectionRequest = decode_body(&body)?;
    let view = OperatorConnectionControl::new(&state)
        .create(&caller, request)
        .await?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// List the tenant's Operator connections, redacted.
///
/// # Errors
/// Returns a stable Wyrd error when the caller lacks `operators:read` or the
/// read fails.
#[utoipa::path(
    get,
    path = "/operator-connections",
    responses(
        (status = 200, description = "Redacted connections", body = Vec<OperatorConnectionView>),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks operators:read \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Operator connections"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "operator_connection.list"))]
async fn list_connections(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<Vec<OperatorConnectionView>>, WyrdErrorResponse> {
    Ok(Json(
        OperatorConnectionControl::new(&state).list(&caller).await?,
    ))
}

/// Read one Operator connection, redacted.
///
/// # Errors
/// Returns a stable Wyrd error when the path is malformed, the caller lacks
/// `operators:read`, the connection is not in the caller's tenant, or the
/// read fails.
#[utoipa::path(
    get,
    path = "/operator-connections/{connection_id}",
    params(("connection_id" = OperatorConnectionId, Path, description = "Connection ID")),
    responses(
        (status = 200, description = "Redacted connection", body = OperatorConnectionView),
        (status = 400, description = "The connection ID is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks operators:read \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such connection in the caller's tenant \
          (WYRD_OPERATOR_404_CONNECTION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Operator connections"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "operator_connection.read"))]
async fn get_connection(
    State(state): State<AppState>,
    caller: Caller,
    connection_id: Result<Path<OperatorConnectionId>, PathRejection>,
) -> Result<Json<OperatorConnectionView>, WyrdErrorResponse> {
    let Path(connection_id) = connection_id.map_err(|rejection| path_rejection(&rejection))?;
    Ok(Json(
        OperatorConnectionControl::new(&state)
            .get(&caller, connection_id)
            .await?,
    ))
}

/// Update one Operator connection's authority, status, or secret.
///
/// # Errors
/// Returns a stable Wyrd error when the path or body is malformed, the
/// provider differs, the caller lacks `operators:write`, the connection is
/// not in the caller's tenant, a supplied secret cannot be sealed, or the
/// write fails.
#[utoipa::path(
    patch,
    path = "/operator-connections/{connection_id}",
    params(("connection_id" = OperatorConnectionId, Path, description = "Connection ID")),
    request_body = UpdateOperatorConnectionRequest,
    responses(
        (status = 200, description = "Updated connection", body = OperatorConnectionView),
        (status = 400, description = "The ID or body is invalid, or the provider differs \
          (WYRD_SPEC_400_VALIDATION, WYRD_OPERATOR_400_INVALID_CONNECTION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks operators:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such connection in the caller's tenant \
          (WYRD_OPERATOR_404_CONNECTION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "No active connection key is readable, or the \
          registry is unavailable (WYRD_OPERATOR_503_KEY_UNAVAILABLE, \
          WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Operator connections"
)]
#[tracing::instrument(
    skip(state, caller, body),
    fields(operation = "operator_connection.update")
)]
async fn update_connection(
    State(state): State<AppState>,
    caller: Caller,
    connection_id: Result<Path<OperatorConnectionId>, PathRejection>,
    body: Bytes,
) -> Result<Json<OperatorConnectionView>, WyrdErrorResponse> {
    let Path(connection_id) = connection_id.map_err(|rejection| path_rejection(&rejection))?;
    let request: UpdateOperatorConnectionRequest = decode_body(&body)?;
    Ok(Json(
        OperatorConnectionControl::new(&state)
            .update(&caller, connection_id, request)
            .await?,
    ))
}

/// Disable one Operator connection; the row is kept and may be re-enabled.
///
/// # Errors
/// Returns a stable Wyrd error when the path is malformed, the caller lacks
/// `operators:write`, the connection is not in the caller's tenant, or the
/// write fails.
#[utoipa::path(
    delete,
    path = "/operator-connections/{connection_id}",
    params(("connection_id" = OperatorConnectionId, Path, description = "Connection ID")),
    responses(
        (status = 200, description = "Disabled connection", body = OperatorConnectionView),
        (status = 400, description = "The connection ID is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks operators:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such connection in the caller's tenant \
          (WYRD_OPERATOR_404_CONNECTION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Operator connections"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "operator_connection.disable"))]
async fn disable_connection(
    State(state): State<AppState>,
    caller: Caller,
    connection_id: Result<Path<OperatorConnectionId>, PathRejection>,
) -> Result<Json<OperatorConnectionView>, WyrdErrorResponse> {
    let Path(connection_id) = connection_id.map_err(|rejection| path_rejection(&rejection))?;
    Ok(Json(
        OperatorConnectionControl::new(&state)
            .disable(&caller, connection_id)
            .await?,
    ))
}
