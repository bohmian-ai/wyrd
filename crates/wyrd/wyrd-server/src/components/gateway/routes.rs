//! HTTP projection of tenant gateway administration.
//!
//! Handlers parse the path name, then delegate to [`GatewayAdministration`],
//! which owns authorization, audit, validation, and persistence.

use std::time::Duration;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::rejection::{BytesRejection, JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use vala_bifrost_redux::catalog::{BifrostCatalogError, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_gateway::{IngressDialect, MediaRequest, OpenAiMediaRoute, ResponseBody};
use wyrd_runtime::{Action, Permission, Resource};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::gateway::openai::{
    GatewayBatch, GatewayBatchCreateRequest, GatewayBatchList, GatewayBatchListQuery,
    GatewayChatCompletion, GatewayChatCompletionsRequest, GatewayEmbeddings,
    GatewayEmbeddingsRequest, GatewayFile, GatewayFileDeleted, GatewayFileUploadForm,
    GatewayFormFile, GatewayImageEditForm, GatewayImageGenerationRequest,
    GatewayImageVariationForm, GatewayImages, GatewayModel, GatewayModelList, GatewayResponse,
    GatewayResponsesRequest, GatewaySpeechRequest, GatewayTranscription, GatewayTranscriptionForm,
    GatewayTranslationForm, OpenAiErrorEnvelope,
};
use wyrd_spec::gateway::{
    GatewayCapturePolicy, GatewayCapturePolicyWrite, GatewayContractError, GatewayDecimal,
    GatewayFallbackPolicy, GatewayGovernancePolicy, GatewayOperation, GatewayUsageAmount, ModelRef,
    ProviderCredentialView, ProviderCredentialWrite, ProviderDeployment,
};
use wyrd_spec::ids::{ProviderCredentialName, ProviderDeploymentName};
use wyrd_storage::StorageError;

use super::batches::{BatchAnswer, GatewayBatches};
use super::capture::object_path;
use super::invocation::{
    GatewayCallRequest, GatewayCallResponse, GatewayInvocation, invalid_request,
};
use super::multipart::{FileSink, FormReader};
use super::service::{GatewayAdministration, invalid, unavailable};
use crate::audit;
use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Deadline of one public gateway call.
pub(super) const PUBLIC_CALL_TIMEOUT: Duration = Duration::from_mins(5);

/// Handler result carrying a stable Wyrd error.
type GatewayResult<T> = Result<T, WyrdErrorResponse>;

/// Builds the tenant gateway administration and payload-object routes for the
/// `/v1` group; inference routes authenticate through
/// [`super::ingress::gateway_ingress_router`].
pub fn gateway_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(gateway_payload_object))
        .routes(routes!(list_credentials))
        .routes(routes!(put_credential, get_credential, delete_credential))
        .routes(routes!(revoke_credential))
        .routes(routes!(list_deployments))
        .routes(routes!(put_deployment, get_deployment, delete_deployment))
        .routes(routes!(put_fallback, get_fallback, delete_fallback))
        .routes(routes!(put_governance, get_governance, delete_governance))
        .routes(routes!(put_capture, get_capture))
}

/// Parses a path segment as a credential name.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for field `name` when invalid.
fn credential_name(raw: &str) -> GatewayResult<ProviderCredentialName> {
    ProviderCredentialName::new(raw).map_err(|_| invalid_name())
}

/// Parses a path segment as a deployment name.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for field `name` when invalid.
fn deployment_name(raw: &str) -> GatewayResult<ProviderDeploymentName> {
    ProviderDeploymentName::new(raw).map_err(|_| invalid_name())
}

/// Decodes a gateway JSON request body through the stable Wyrd error contract.
///
/// Axum's bare `Json` rejection would bypass the Wyrd problem mapper, so the
/// handlers extract `Result<Json<T>, JsonRejection>` after authentication and
/// convert malformed or shape-invalid bodies here. The rejection text is not
/// echoed because it may quote submitted secret-bearing input.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for field `body` when the body is
/// missing, not JSON, or not the expected contract shape.
fn body<T>(extracted: Result<Json<T>, JsonRejection>) -> GatewayResult<T> {
    extracted.map(|Json(value)| value).map_err(|_| {
        invalid(GatewayContractError::new(
            "body",
            "must be a JSON document matching the resource contract",
        ))
        .into()
    })
}

/// Audited operation name of a captured payload object retrieval.
const PAYLOAD_OBJECT_OPERATION: &str = "gateway.payload_object.get";

/// Stable absence of a captured payload object.
///
/// A non-canonical digest, another tenant's object, one that was never
/// captured, and one the storage bucket's lifecycle has already expired all
/// produce this single outcome, so a caller can distinguish none of them. It
/// never names the server-derived storage key.
fn object_not_found(digest: &str) -> WyrdErrorResponse {
    WyrdError::GatewayResourceNotFound {
        message: "captured payload object not found in tenant".to_owned(),
        details: json!({ "resource": "payload_object", "digest": digest }),
    }
    .into()
}

#[utoipa::path(
    get,
    path = "/gateway/payload-objects/{digest}",
    params(("digest" = String, Path, description = "Canonical sha256: digest of the captured object")),
    responses(
        (status = 200, description = "Exact captured bytes", content_type = "application/octet-stream", body = GatewayFormFile),
        (status = 404, description = "Object not found in tenant (WYRD_GATEWAY_404_RESOURCE_NOT_FOUND)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Payload and captured-table read permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Catalog or storage unavailable, or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/gateway/payload-objects/{digest}`.
///
/// Returns the exact bytes a captured call's typed reference names.
///
/// The tenant is taken only from the verified principal, never from the path,
/// and the storage key is derived from that tenant and the canonical digest, so
/// a digest belonging to another tenant simply resolves to an absent object.
/// Retrieval requires both tenant-wide `gateway_payload:read` and query access
/// scoped to this tenant's registered `vala.gateway.calls`, and each decision
/// is audited through the canonical path before the bytes are read. The table
/// UID is resolved by lookup alone, so a read provisions nothing. Because the
/// bucket lifecycle owns expiration, a reference can outlive its object; that
/// case returns the same stable not-found outcome as any absent object.
///
/// The returned bytes are hashed and must equal the requested digest, so
/// content replaced under a valid key is never served.
///
/// # Errors
/// Returns `GatewayResourceNotFound` for a non-canonical digest, a tenant with
/// no registered calls table, and an absent or expired object; the mapped
/// permission denial when either grant is missing; and the stable
/// `ServiceUnavailable` for audit, catalog, or storage failure and for stored
/// bytes that do not match the digest, without naming any storage locator.
#[tracing::instrument(skip_all, fields(operation = PAYLOAD_OBJECT_OPERATION))]
pub(crate) async fn gateway_payload_object(
    State(state): State<AppState>,
    caller: Caller,
    Path(digest): Path<String>,
) -> GatewayResult<Response> {
    let tenant = caller.data_tenant_id;
    let path = object_path(tenant, &digest).ok_or_else(|| object_not_found(&digest))?;
    let resource = format!("payload_object:{digest}");
    audit::authorize(
        &state,
        &caller,
        &Permission::gateway_payload_read(),
        PAYLOAD_OBJECT_OPERATION,
        &resource,
    )
    .await?;
    let table = TableRef::new(BifrostNamespace::Gateway, "calls");
    let table_uid = state
        .bifrost
        .catalog()
        .ok_or_else(|| unavailable("this replica has no Bifrost catalog"))?
        .table_uid(&table, tenant)
        .await
        .map_err(|error| match error {
            BifrostCatalogError::TableNotFound(_) => object_not_found(&digest),
            other => unavailable(other).into(),
        })?;
    let scoped = Permission {
        resource: Resource::BifrostQuery,
        action: Action::Read,
        scope: table.permission_scope(&table_uid),
    };
    audit::authorize(
        &state,
        &caller,
        &scoped,
        PAYLOAD_OBJECT_OPERATION,
        &resource,
    )
    .await?;
    let bytes = state
        .storage
        .get_object(&path)
        .await
        .map_err(|error| match error {
            StorageError::ObjectNotFound { .. } => object_not_found(&digest),
            other => unavailable(other).into(),
        })?;
    if format!("sha256:{}", hex::encode(Sha256::digest(&bytes))) != digest {
        return Err(unavailable("stored payload object does not match its digest").into());
    }
    let length = bytes.len().to_string();
    Ok((
        StatusCode::OK,
        [
            (CONTENT_TYPE, "application/octet-stream"),
            (CONTENT_LENGTH, length.as_str()),
        ],
        Body::from(bytes),
    )
        .into_response())
}

/// Stable rejection for an invalid path name.
fn invalid_name() -> WyrdErrorResponse {
    invalid(GatewayContractError::new(
        "name",
        "is not a valid resource name",
    ))
    .into()
}

#[utoipa::path(
    put,
    path = "/admin/gateway/provider-credentials/{name}",
    params(("name" = String, Path, description = "Tenant-scoped resource name")),
    request_body = ProviderCredentialWrite,
    responses(
        (status = 200, body = ProviderCredentialView),
        (status = 400, description = "Invalid name or credential document (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 409, description = "Credential is revoked or source changed (WYRD_GATEWAY_409_RESOURCE_CONFLICT)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `PUT /v1/admin/gateway/provider-credentials/{name}`.
///
/// Creates or rotates one tenant credential source; persists only the redacted
/// source and audits the decision.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, body, or source,
/// `GatewayResourceConflict` for a revoked name or a provider change that
/// would orphan a deployment, and authentication, permission, storage, or audit errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.provider_credential.put"))]
pub(crate) async fn put_credential(
    State(state): State<AppState>,
    caller: Caller,
    Path(name): Path<String>,
    write: Result<Json<ProviderCredentialWrite>, JsonRejection>,
) -> GatewayResult<Json<ProviderCredentialView>> {
    let name = credential_name(&name)?;
    let write = body(write)?;
    Ok(Json(
        GatewayAdministration::new(&state)
            .put_credential(&caller, &name, write)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/admin/gateway/provider-credentials/{name}",
    params(("name" = String, Path, description = "Tenant-scoped resource name")),
    responses(
        (status = 200, body = ProviderCredentialView),
        (status = 400, description = "Invalid name (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 404, description = "Credential not found (WYRD_GATEWAY_404_RESOURCE_NOT_FOUND)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/provider-credentials/{name}`.
///
/// Reads one redacted credential view.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, `GatewayResourceNotFound`
/// when absent, and authentication, permission, storage, or audit errors.
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "gateway.provider_credential.get")
)]
pub(crate) async fn get_credential(
    State(state): State<AppState>,
    caller: Caller,
    Path(name): Path<String>,
) -> GatewayResult<Json<ProviderCredentialView>> {
    let name = credential_name(&name)?;
    Ok(Json(
        GatewayAdministration::new(&state)
            .credential(&caller, &name)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/admin/gateway/provider-credentials",
    responses(
        (status = 200, body = Vec<ProviderCredentialView>),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/provider-credentials`.
///
/// Lists the tenant's redacted credential views ordered by name.
///
/// # Errors
/// Returns authentication, permission, storage, or audit errors.
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "gateway.provider_credential.list")
)]
pub(crate) async fn list_credentials(
    State(state): State<AppState>,
    caller: Caller,
) -> GatewayResult<Json<Vec<ProviderCredentialView>>> {
    Ok(Json(
        GatewayAdministration::new(&state)
            .credentials(&caller)
            .await?,
    ))
}

#[utoipa::path(
    post,
    path = "/admin/gateway/provider-credentials/{name}/revoke",
    params(("name" = String, Path, description = "Tenant-scoped resource name")),
    responses(
        (status = 200, body = ProviderCredentialView),
        (status = 400, description = "Invalid name (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 404, description = "Credential not found (WYRD_GATEWAY_404_RESOURCE_NOT_FOUND)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `POST /v1/admin/gateway/provider-credentials/{name}/revoke`.
///
/// Terminally revokes one credential; repeating returns the same revocation.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, `GatewayResourceNotFound`
/// when absent, and authentication, permission, storage, or audit errors.
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "gateway.provider_credential.revoke")
)]
pub(crate) async fn revoke_credential(
    State(state): State<AppState>,
    caller: Caller,
    Path(name): Path<String>,
) -> GatewayResult<Json<ProviderCredentialView>> {
    let name = credential_name(&name)?;
    Ok(Json(
        GatewayAdministration::new(&state)
            .revoke_credential(&caller, &name)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/admin/gateway/provider-credentials/{name}",
    params(("name" = String, Path, description = "Tenant-scoped resource name")),
    responses(
        (status = 204, description = "Deleted or already absent"),
        (status = 400, description = "Invalid name (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 409, description = "Credential is referenced (WYRD_GATEWAY_409_RESOURCE_CONFLICT)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `DELETE /v1/admin/gateway/provider-credentials/{name}`.
///
/// Deletes an unreferenced credential; an absent name succeeds with 204.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, `GatewayResourceConflict`
/// while a deployment references it, and authentication, permission, storage, or audit errors.
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "gateway.provider_credential.delete")
)]
pub(crate) async fn delete_credential(
    State(state): State<AppState>,
    caller: Caller,
    Path(name): Path<String>,
) -> GatewayResult<StatusCode> {
    let name = credential_name(&name)?;
    GatewayAdministration::new(&state)
        .delete_credential(&caller, &name)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/admin/gateway/provider-deployments/{name}",
    params(("name" = String, Path, description = "Tenant-scoped resource name")),
    request_body = ProviderDeployment,
    responses(
        (status = 200, body = ProviderDeployment),
        (status = 400, description = "Invalid name, deployment document, or credential reference (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `PUT /v1/admin/gateway/provider-deployments/{name}`.
///
/// Creates or replaces one deployment; only newly admitted calls observe it.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, document, or credential
/// reference, and authentication, permission, storage, or audit errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.provider_deployment.put"))]
pub(crate) async fn put_deployment(
    State(state): State<AppState>,
    caller: Caller,
    Path(name): Path<String>,
    deployment: Result<Json<ProviderDeployment>, JsonRejection>,
) -> GatewayResult<Json<ProviderDeployment>> {
    let name = deployment_name(&name)?;
    let deployment = body(deployment)?;
    Ok(Json(
        GatewayAdministration::new(&state)
            .put_deployment(&caller, &name, deployment)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/admin/gateway/provider-deployments/{name}",
    params(("name" = String, Path, description = "Tenant-scoped resource name")),
    responses(
        (status = 200, body = ProviderDeployment),
        (status = 400, description = "Invalid name (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 404, description = "Deployment not found (WYRD_GATEWAY_404_RESOURCE_NOT_FOUND)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/provider-deployments/{name}`.
///
/// Reads one deployment.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, `GatewayResourceNotFound`
/// when absent, and authentication, permission, storage, or audit errors.
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "gateway.provider_deployment.get")
)]
pub(crate) async fn get_deployment(
    State(state): State<AppState>,
    caller: Caller,
    Path(name): Path<String>,
) -> GatewayResult<Json<ProviderDeployment>> {
    let name = deployment_name(&name)?;
    Ok(Json(
        GatewayAdministration::new(&state)
            .deployment(&caller, &name)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/admin/gateway/provider-deployments",
    responses(
        (status = 200, body = Vec<ProviderDeployment>),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/provider-deployments`.
///
/// Lists the tenant's deployments ordered by name.
///
/// # Errors
/// Returns authentication, permission, storage, or audit errors.
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "gateway.provider_deployment.list")
)]
pub(crate) async fn list_deployments(
    State(state): State<AppState>,
    caller: Caller,
) -> GatewayResult<Json<Vec<ProviderDeployment>>> {
    Ok(Json(
        GatewayAdministration::new(&state)
            .deployments(&caller)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/admin/gateway/provider-deployments/{name}",
    params(("name" = String, Path, description = "Tenant-scoped resource name")),
    responses(
        (status = 204, description = "Deleted or already absent"),
        (status = 400, description = "Invalid name (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `DELETE /v1/admin/gateway/provider-deployments/{name}`.
///
/// Deletes one deployment; an absent name succeeds with 204.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, and authentication, permission, storage, or audit errors.
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "gateway.provider_deployment.delete")
)]
pub(crate) async fn delete_deployment(
    State(state): State<AppState>,
    caller: Caller,
    Path(name): Path<String>,
) -> GatewayResult<StatusCode> {
    let name = deployment_name(&name)?;
    GatewayAdministration::new(&state)
        .delete_deployment(&caller, &name)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/admin/gateway/fallback-policy",
    request_body = GatewayFallbackPolicy,
    responses(
        (status = 200, body = GatewayFallbackPolicy),
        (status = 400, description = "Invalid fallback policy (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `PUT /v1/admin/gateway/fallback-policy`.
///
/// Atomically replaces the tenant fallback policy for newly admitted calls.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid body or policy, and
/// authentication, permission, storage, or audit errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.fallback_policy.put"))]
pub(crate) async fn put_fallback(
    State(state): State<AppState>,
    caller: Caller,
    policy: Result<Json<GatewayFallbackPolicy>, JsonRejection>,
) -> GatewayResult<Json<GatewayFallbackPolicy>> {
    let policy = body(policy)?;
    Ok(Json(
        GatewayAdministration::new(&state)
            .put_fallback(&caller, policy)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/admin/gateway/fallback-policy",
    responses(
        (status = 200, body = GatewayFallbackPolicy),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/fallback-policy`.
///
/// Reads the tenant fallback policy, or the empty default.
///
/// # Errors
/// Returns authentication, permission, storage, or audit errors.
#[tracing::instrument(skip(state, caller), fields(operation = "gateway.fallback_policy.get"))]
pub(crate) async fn get_fallback(
    State(state): State<AppState>,
    caller: Caller,
) -> GatewayResult<Json<GatewayFallbackPolicy>> {
    Ok(Json(
        GatewayAdministration::new(&state).fallback(&caller).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/admin/gateway/fallback-policy",
    responses(
        (status = 204, description = "Deleted or already absent"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `DELETE /v1/admin/gateway/fallback-policy`.
///
/// Restores the empty fallback policy; repeating succeeds.
///
/// # Errors
/// Returns authentication, permission, storage, or audit errors.
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "gateway.fallback_policy.delete")
)]
pub(crate) async fn delete_fallback(
    State(state): State<AppState>,
    caller: Caller,
) -> GatewayResult<StatusCode> {
    GatewayAdministration::new(&state)
        .delete_fallback(&caller)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/admin/gateway/governance-policy",
    request_body = GatewayGovernancePolicy,
    responses(
        (status = 200, body = GatewayGovernancePolicy),
        (status = 400, description = "Invalid governance policy or changed pricing version (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `PUT /v1/admin/gateway/governance-policy`.
///
/// Atomically replaces governance, retaining omitted pricing versions inactive.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid body or policy or a changed
/// pricing version, and authentication, permission, storage, or audit errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.governance_policy.put"))]
pub(crate) async fn put_governance(
    State(state): State<AppState>,
    caller: Caller,
    policy: Result<Json<GatewayGovernancePolicy>, JsonRejection>,
) -> GatewayResult<Json<GatewayGovernancePolicy>> {
    let policy = body(policy)?;
    Ok(Json(
        GatewayAdministration::new(&state)
            .put_governance(&caller, policy)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/admin/gateway/governance-policy",
    responses(
        (status = 200, body = GatewayGovernancePolicy),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/governance-policy`.
///
/// Reads the tenant governance policy with its pricing history.
///
/// # Errors
/// Returns authentication, permission, storage, or audit errors.
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "gateway.governance_policy.get")
)]
pub(crate) async fn get_governance(
    State(state): State<AppState>,
    caller: Caller,
) -> GatewayResult<Json<GatewayGovernancePolicy>> {
    Ok(Json(
        GatewayAdministration::new(&state)
            .governance(&caller)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/admin/gateway/governance-policy",
    responses(
        (status = 204, description = "Deleted or already absent"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `DELETE /v1/admin/gateway/governance-policy`.
///
/// Clears limits and budgets and retires pricing; repeating succeeds.
///
/// # Errors
/// Returns authentication, permission, storage, or audit errors.
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "gateway.governance_policy.delete")
)]
pub(crate) async fn delete_governance(
    State(state): State<AppState>,
    caller: Caller,
) -> GatewayResult<StatusCode> {
    GatewayAdministration::new(&state)
        .delete_governance(&caller)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/admin/gateway/capture-policy",
    request_body = GatewayCapturePolicyWrite,
    responses(
        (status = 200, body = GatewayCapturePolicy),
        (status = 400, description = "Invalid capture policy (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `PUT /v1/admin/gateway/capture-policy`.
///
/// Replaces the capture policy, bumping its version only on a content change.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid body or mode/field combination,
/// and authentication, permission, storage, or audit errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.capture_policy.put"))]
pub(crate) async fn put_capture(
    State(state): State<AppState>,
    caller: Caller,
    write: Result<Json<GatewayCapturePolicyWrite>, JsonRejection>,
) -> GatewayResult<Json<GatewayCapturePolicy>> {
    let write = body(write)?;
    Ok(Json(
        GatewayAdministration::new(&state)
            .put_capture(&caller, write)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/admin/gateway/capture-policy",
    responses(
        (status = 200, body = GatewayCapturePolicy),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Administration store or token verification unavailable (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal, including an unauditable authorization decision (WYRD_SPEC_500_INTERNAL, WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/capture-policy`.
///
/// Reads the capture policy, or the disabled version-1 default.
///
/// # Errors
/// Returns authentication, permission, storage, or audit errors.
#[tracing::instrument(skip(state, caller), fields(operation = "gateway.capture_policy.get"))]
pub(crate) async fn get_capture(
    State(state): State<AppState>,
    caller: Caller,
) -> GatewayResult<Json<GatewayCapturePolicy>> {
    Ok(Json(
        GatewayAdministration::new(&state).capture(&caller).await?,
    ))
}

#[utoipa::path(
    post,
    path = "/v1/chat/completions",
    request_body(content = GatewayChatCompletionsRequest, description = "OpenAI-compatible chat completion request whose `model` is an exact `<provider>/<model>` projection"),
    responses(
        (status = 200, description = "Chat completion response as JSON, or server-sent events when `stream` is true", content(
            (GatewayChatCompletion = "application/json"),
            (String = "text/event-stream")
        )),
        (status = 400, description = "Invalid gateway request, or a feature no authorized deployment can represent, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/chat/completions`: one governed chat completion.
///
/// Tenant and principal come only from the verified caller; the body names the
/// model and reaches the provider unchanged or through faithful translation.
/// See [`openai_call`] for errors, buffering, and streaming.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn chat_completions(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    openai_call(
        &state,
        caller,
        typed_body::<GatewayChatCompletionsRequest>(request).map(|body| (body, None)),
        GatewayOperation::ChatCompletions,
    )
    .await
}

#[utoipa::path(
    post,
    path = "/v1/responses",
    request_body(content = GatewayResponsesRequest, description = "OpenAI-compatible Responses request whose `model` is an exact `<provider>/<model>` projection"),
    responses(
        (status = 200, description = "Response object as JSON, or server-sent events when `stream` is true", content(
            (GatewayResponse = "application/json"),
            (String = "text/event-stream")
        )),
        (status = 400, description = "Invalid gateway request, or a feature no authorized deployment can represent, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/responses`: one governed Responses call, served only by
/// deployments that speak the `OpenAI` Responses protocol.
///
/// See [`openai_call`] for errors, buffering, and streaming.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn responses(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    openai_call(
        &state,
        caller,
        typed_body::<GatewayResponsesRequest>(request).map(|body| (body, None)),
        GatewayOperation::Responses,
    )
    .await
}

#[utoipa::path(
    post,
    path = "/v1/embeddings",
    request_body(content = GatewayEmbeddingsRequest, description = "OpenAI-compatible embeddings request whose `model` is an exact `<provider>/<model>` projection"),
    responses(
        (status = 200, description = "Embeddings in input order with usage, as JSON", body = GatewayEmbeddings),
        (status = 400, description = "Invalid gateway request, including `stream`, or a feature no authorized deployment can represent, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/embeddings`: one governed embeddings call. Batch input,
/// `dimensions`, and per-item order reach an `OpenAI`-protocol provider
/// unchanged; other adapters reject embeddings before dispatch.
///
/// See [`openai_call`] for errors and buffering.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn embeddings(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    openai_call(
        &state,
        caller,
        typed_body::<GatewayEmbeddingsRequest>(request).map(|body| (body, None)),
        GatewayOperation::Embeddings,
    )
    .await
}

#[utoipa::path(
    post,
    path = "/v1/images/generations",
    request_body(content = GatewayImageGenerationRequest, description = "OpenAI-compatible image generation request whose `model` is an exact `<provider>/<model>` projection; `stream` is not accepted"),
    responses(
        (status = 200, description = "Generated images as the provider's JSON, with base64 data or URLs", body = GatewayImages),
        (status = 400, description = "Invalid gateway request, or an operation no authorized deployment can represent, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves Images for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed or the answer exceeded its bound, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/images/generations`: one governed image generation, served only
/// by `OpenAI`-protocol deployments that declare Images.
///
/// Media calls never retry after dispatch. See [`openai_call`] for errors and
/// buffering.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn image_generations(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    json_media(&state, caller, request, OpenAiMediaRoute::ImageGenerations).await
}

#[utoipa::path(
    post,
    path = "/v1/images/edits",
    request_body(content = GatewayImageEditForm, content_type = "multipart/form-data", description = "OpenAI-compatible image edit form: a `model` field with an exact `<provider>/<model>` projection, one to sixteen `image` or `image[]` files, and an optional `mask` file"),
    responses(
        (status = 200, description = "Edited images as the provider's JSON", body = GatewayImages),
        (status = 400, description = "Malformed form, missing or unexpected files, or an operation no authorized deployment can represent, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves Images for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 413, description = "Form exceeds the request body limit (WYRD_SPEC_413_PAYLOAD_TOO_LARGE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 422, description = "Call cost cannot be bounded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed or the answer exceeded its bound, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/images/edits`: one governed image edit from a bounded form.
///
/// See [`openai_call`] for errors and buffering.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn image_edits(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: HeaderMap,
    request: Result<Bytes, BytesRejection>,
) -> Response {
    let request = form_body(
        OpenAiMediaRoute::ImageEdits,
        &headers,
        request.map(Body::from),
        state.limits.body_bytes,
        FileSink::Memory,
    )
    .await;
    openai_call(&state, caller, request, GatewayOperation::Images).await
}

#[utoipa::path(
    post,
    path = "/v1/images/variations",
    request_body(content = GatewayImageVariationForm, content_type = "multipart/form-data", description = "OpenAI-compatible image variation form: a `model` field with an exact `<provider>/<model>` projection and exactly one `image` file"),
    responses(
        (status = 200, description = "Image variations as the provider's JSON", body = GatewayImages),
        (status = 400, description = "Malformed form, missing or unexpected files, or an operation no authorized deployment can represent, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves Images for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 413, description = "Form exceeds the request body limit (WYRD_SPEC_413_PAYLOAD_TOO_LARGE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 422, description = "Call cost cannot be bounded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed or the answer exceeded its bound, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/images/variations`: one governed image variation from a bounded
/// form.
///
/// See [`openai_call`] for errors and buffering.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn image_variations(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: HeaderMap,
    request: Result<Bytes, BytesRejection>,
) -> Response {
    let request = form_body(
        OpenAiMediaRoute::ImageVariations,
        &headers,
        request.map(Body::from),
        state.limits.body_bytes,
        FileSink::Memory,
    )
    .await;
    openai_call(&state, caller, request, GatewayOperation::Images).await
}

#[utoipa::path(
    post,
    path = "/v1/audio/speech",
    request_body(content = GatewaySpeechRequest, description = "OpenAI-compatible speech request whose `model` is an exact `<provider>/<model>` projection; `stream` is not accepted"),
    responses(
        (status = 200, description = "Audio bytes in the provider's content type, such as audio/mpeg", content(
            (String = "application/octet-stream")
        )),
        (status = 400, description = "Invalid gateway request, or an operation no authorized deployment can represent, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves Audio for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed or the answer exceeded its bound, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/audio/speech`: one governed speech generation whose bounded
/// audio returns with the provider's content type.
///
/// See [`openai_call`] for errors and buffering.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn audio_speech(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    json_media(&state, caller, request, OpenAiMediaRoute::AudioSpeech).await
}

#[utoipa::path(
    post,
    path = "/v1/audio/transcriptions",
    request_body(content = GatewayTranscriptionForm, content_type = "multipart/form-data", description = "OpenAI-compatible transcription form: a `model` field with an exact `<provider>/<model>` projection and exactly one `file`"),
    responses(
        (status = 200, description = "Transcript as JSON, or text in the provider's content type for text formats", content(
            (GatewayTranscription = "application/json"),
            (String = "text/plain")
        )),
        (status = 400, description = "Malformed form, missing or unexpected files, or an operation no authorized deployment can represent, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves Audio for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 413, description = "Form exceeds the Audio upload limit, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed or the answer exceeded its bound, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/audio/transcriptions`: one governed transcription of a streamed
/// upload bounded by `limits.audio_upload_bytes`.
///
/// See [`audio_form`] for streaming and [`openai_call`] for errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn audio_transcriptions(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    audio_form(
        &state,
        caller,
        &headers,
        body,
        OpenAiMediaRoute::AudioTranscriptions,
    )
    .await
}

#[utoipa::path(
    post,
    path = "/v1/audio/translations",
    request_body(content = GatewayTranslationForm, content_type = "multipart/form-data", description = "OpenAI-compatible translation form: a `model` field with an exact `<provider>/<model>` projection and exactly one `file`"),
    responses(
        (status = 200, description = "English translation as JSON, or text in the provider's content type for text formats", content(
            (GatewayTranscription = "application/json"),
            (String = "text/plain")
        )),
        (status = 400, description = "Malformed form, missing or unexpected files, or an operation no authorized deployment can represent, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves Audio for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 413, description = "Form exceeds the Audio upload limit, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed or the answer exceeded its bound, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/audio/translations`: one governed translation of a streamed
/// upload bounded by `limits.audio_upload_bytes`.
///
/// See [`audio_form`] for streaming and [`openai_call`] for errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn audio_translations(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    audio_form(
        &state,
        caller,
        &headers,
        body,
        OpenAiMediaRoute::AudioTranslations,
    )
    .await
}

/// Runs an Audio upload `route` whose form streams from `body`.
///
/// The router does not buffer these bodies. An authenticated caller's form is
/// read chunk by chunk under `limits.audio_upload_bytes`, its file spooled to
/// an anonymous temporary file that the provider request streams from; an
/// unauthenticated body is never read. Nothing dispatches until the whole
/// form has arrived, so an oversized or abandoned upload never reaches a
/// provider.
///
/// See [`openai_call`] for errors.
async fn audio_form(
    state: &AppState,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: &HeaderMap,
    body: Body,
    route: OpenAiMediaRoute,
) -> Response {
    let request = match &caller {
        Ok(_) => {
            form_body(
                route,
                headers,
                Ok(body),
                state.limits.audio_upload_bytes,
                FileSink::Spool,
            )
            .await
        }
        // `openai_call` answers with the caller's error before the request.
        Err(_) => Err(invalid_request(GatewayContractError::new(
            "body",
            "was not read",
        ))),
    };
    openai_call(state, caller, request, GatewayOperation::Audio).await
}

/// Runs a JSON media `route` of its operation family, whose body must decode
/// as the route's shared request contract.
///
/// See [`openai_call`] for errors and buffering.
async fn json_media(
    state: &AppState,
    caller: Result<Caller, WyrdErrorResponse>,
    request: Result<Json<Value>, JsonRejection>,
    route: OpenAiMediaRoute,
) -> Response {
    let (operation, body) = match route {
        OpenAiMediaRoute::AudioSpeech => (
            GatewayOperation::Audio,
            typed_body::<GatewaySpeechRequest>(request),
        ),
        _ => (
            GatewayOperation::Images,
            typed_body::<GatewayImageGenerationRequest>(request),
        ),
    };
    let media = MediaRequest {
        route,
        files: Vec::new(),
    };
    openai_call(
        state,
        caller,
        body.map(|body| (body, Some(media))),
        operation,
    )
    .await
}

#[utoipa::path(
    get,
    path = "/v1/models",
    responses(
        (status = 200, description = "OpenAI model list of the exact `<provider>/<model>` projections configured for the tenant that the caller may invoke", body = GatewayModelList),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or configuration or audit is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `GET /v1/models`: the `OpenAI` model list of exact projections configured
/// for the caller's tenant and visible to the caller.
///
/// Each entry's `owned_by` is its provider; `created` is `0` because the
/// gateway records no provider release time. Failures render as the `OpenAI`
/// error envelope.
#[tracing::instrument(skip_all, fields(operation = "gateway.models.list"))]
pub(crate) async fn models(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
) -> Response {
    let listed = match caller {
        Ok(caller) => GatewayInvocation::new(&state).models(&caller).await,
        Err(WyrdErrorResponse(error)) => Err(error),
    };
    match listed {
        Ok(models) => Json(GatewayModelList {
            object: "list".to_owned(),
            data: models
                .iter()
                .map(|model| GatewayModel {
                    id: format!("{}/{}", model.provider.as_str(), model.model.as_str()),
                    object: "model".to_owned(),
                    created: 0,
                    owned_by: model.provider.as_str().to_owned(),
                })
                .collect(),
        })
        .into_response(),
        Err(error) => openai_error(&error),
    }
}

/// Runs one `OpenAI`-compatible `operation` from its decoded body and media
/// parts, and renders its answer.
///
/// Gateway-originated failures, authentication included, render as the
/// `OpenAI` error envelope carrying the stable Wyrd code; authentication is
/// reported before a malformed body. A provider refusal keeps the provider's
/// status. A buffered answer relays the provider's bytes (or its faithful
/// translation) as JSON; a non-JSON media answer relays its bytes with the
/// provider's content type; `stream: true` answers with server-sent events
/// relayed incrementally, and a stream that ends without its terminator or
/// terminal error aborts the response. The request-id middleware correlates
/// every response through the `wyrd-request-id` header.
async fn openai_call(
    state: &AppState,
    caller: Result<Caller, WyrdErrorResponse>,
    request: Result<(Value, Option<MediaRequest>), WyrdError>,
    operation: GatewayOperation,
) -> Response {
    let result = match caller {
        Ok(caller) => invoke_openai(state, &caller, request, operation).await,
        Err(WyrdErrorResponse(error)) => Err(error),
    };
    match result {
        Ok(response) => relay(response.status, response.body),
        Err(error) => openai_error(&error),
    }
}

/// Renders a provider `status` and answer `body`: JSON as JSON, media bytes
/// with their content type, and events as an incremental server-sent event
/// stream.
pub(super) fn relay(status: u16, body: ResponseBody) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    match body {
        ResponseBody::Json(raw) => (
            status,
            [(CONTENT_TYPE, "application/json")],
            Box::<str>::from(raw).into_string(),
        )
            .into_response(),
        ResponseBody::Media(answer) => {
            let content_type = HeaderValue::from_str(&answer.content_type)
                .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
            (status, [(CONTENT_TYPE, content_type)], answer.bytes).into_response()
        }
        ResponseBody::MediaStream {
            content_type,
            events,
        } => {
            let content_type = HeaderValue::from_str(&content_type)
                .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
            (status, [(CONTENT_TYPE, content_type)], events_body(events)).into_response()
        }
        ResponseBody::Events(events) => (
            status,
            [(CONTENT_TYPE, "text/event-stream")],
            events_body(events),
        )
            .into_response(),
    }
}

/// Response body relaying `events` as the relay produces them.
///
/// Dropping the body drops the receiver, which aborts the upstream request. An
/// unterminated stream yields an error, which aborts the response.
fn events_body(events: wyrd_gateway::EventStream) -> Body {
    Body::from_stream(futures_util::stream::unfold(
        events,
        |mut events| async move {
            let frame = events.recv().await?;
            Some((frame, events))
        },
    ))
}

#[utoipa::path(
    post,
    path = "/v1/files",
    request_body(content = GatewayFileUploadForm, content_type = "multipart/form-data", description = "OpenAI-compatible file upload form: `purpose` `batch` and exactly one batch JSONL `file` whose request lines all target one endpoint and one exact `<provider>/<model>` projection"),
    responses(
        (status = 200, description = "OpenAI file object with a Wyrd file id", body = GatewayFile),
        (status = 400, description = "Malformed form or batch input file, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves Batches for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 413, description = "Form exceeds the request body limit (WYRD_SPEC_413_PAYLOAD_TOO_LARGE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage or audit is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/files`: uploads one batch input file to a deployment serving its
/// model.
///
/// See [`GatewayBatches::upload`] for errors; failures render as the `OpenAI`
/// error envelope.
#[tracing::instrument(skip_all, fields(operation = "gateway.files.upload"))]
pub(crate) async fn upload_file(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: HeaderMap,
    request: Result<Bytes, BytesRejection>,
) -> Response {
    let answer = match (caller, request) {
        (Err(WyrdErrorResponse(error)), _) => Err(error),
        (Ok(_), Err(_)) => Err(invalid_request(GatewayContractError::new(
            "body",
            "could not be read",
        ))),
        (Ok(caller), Ok(bytes)) => {
            let content_type = headers
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok());
            GatewayBatches::new(&state)
                .upload(&caller, content_type, bytes)
                .await
        }
    };
    batch_response(answer)
}

#[utoipa::path(
    get,
    path = "/v1/files/{file_id}",
    params(("file_id" = String, Path, description = "Wyrd id of an uploaded batch input file")),
    responses(
        (status = 200, description = "OpenAI file object", body = GatewayFile),
        (status = 404, description = "Unknown file, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage or audit is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `GET /v1/files/{file_id}`: reads an uploaded file's stored metadata.
///
/// See [`GatewayBatches::file`] for errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.files.read"))]
pub(crate) async fn get_file(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    Path(file_id): Path<String>,
) -> Response {
    let answer = match caller {
        Ok(caller) => GatewayBatches::new(&state).file(&caller, &file_id).await,
        Err(WyrdErrorResponse(error)) => Err(error),
    };
    batch_response(answer)
}

#[utoipa::path(
    get,
    path = "/v1/files/{file_id}/content",
    params(("file_id" = String, Path, description = "Wyrd id of an uploaded file, or of a batch output or error file")),
    responses(
        (status = 200, description = "File bytes in the provider's content type", content(
            (String = "application/octet-stream")
        )),
        (status = 404, description = "Unknown file, or a batch file not yet observed, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed or the content exceeded its bound, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage or audit is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `GET /v1/files/{file_id}/content`: relays a file's bounded content from
/// the deployment holding it.
///
/// See [`GatewayBatches::file_content`] for errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn file_content(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    Path(file_id): Path<String>,
) -> Response {
    let answer = match caller {
        Ok(caller) => {
            GatewayBatches::new(&state)
                .file_content(&caller, &file_id)
                .await
        }
        Err(WyrdErrorResponse(error)) => Err(error),
    };
    batch_response(answer)
}

#[utoipa::path(
    delete,
    path = "/v1/files/{file_id}",
    params(("file_id" = String, Path, description = "Wyrd id of an uploaded batch input file")),
    responses(
        (status = 200, description = "OpenAI file deletion object", body = GatewayFileDeleted),
        (status = 404, description = "Unknown file, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage or audit is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `DELETE /v1/files/{file_id}`: deletes an uploaded file from its
/// deployment and the tenant.
///
/// See [`GatewayBatches::delete_file`] for errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn delete_file(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    Path(file_id): Path<String>,
) -> Response {
    let answer = match caller {
        Ok(caller) => {
            GatewayBatches::new(&state)
                .delete_file(&caller, &file_id)
                .await
        }
        Err(WyrdErrorResponse(error)) => Err(error),
    };
    batch_response(answer)
}

#[utoipa::path(
    post,
    path = "/v1/batches",
    request_body(content = GatewayBatchCreateRequest, description = "OpenAI-compatible batch creation: a Wyrd `input_file_id`, the file's `endpoint`, `completion_window`, and optional `metadata`; an identical request converges on one batch"),
    responses(
        (status = 200, description = "OpenAI batch object with Wyrd batch and file ids", body = GatewayBatch),
        (status = 400, description = "Invalid request or an endpoint other than the file's, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "Unknown input file, or its deployment no longer serves Batches, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 409, description = "An identical creation is still pending, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 422, description = "Call cost cannot be bounded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 429, description = "Limit or budget exhausted, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 504, description = "Deadline exceeded, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage or audit is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/batches`: creates a batch at most once per canonical request.
///
/// See [`GatewayBatches::create`] for errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn create_batch(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    request: Result<Json<Value>, JsonRejection>,
) -> Response {
    let answer = match (caller, typed_body::<GatewayBatchCreateRequest>(request)) {
        (Err(WyrdErrorResponse(error)), _) | (Ok(_), Err(error)) => Err(error),
        (Ok(caller), Ok(body)) => GatewayBatches::new(&state).create(&caller, body).await,
    };
    batch_response(answer)
}

#[utoipa::path(
    get,
    path = "/v1/batches",
    params(GatewayBatchListQuery),
    responses(
        (status = 200, description = "OpenAI batch list, newest first, of batches whose model the caller may invoke, with their last observed status", body = GatewayBatchList),
        (status = 400, description = "Invalid cursor or limit, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage or audit is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `GET /v1/batches`: lists the tenant's visible batches.
///
/// See [`GatewayBatches::list`] for errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.batches.list"))]
pub(crate) async fn list_batches(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    query: Result<Query<GatewayBatchListQuery>, QueryRejection>,
) -> Response {
    let answer = match (caller, query) {
        (Err(WyrdErrorResponse(error)), _) => Err(error),
        (Ok(_), Err(_)) => Err(invalid_request(GatewayContractError::new(
            "query",
            "must carry an optional after id and a numeric limit",
        ))),
        (Ok(caller), Ok(Query(query))) => {
            GatewayBatches::new(&state)
                .list(&caller, query.after.as_deref(), query.limit)
                .await
        }
    };
    batch_response(answer)
}

#[utoipa::path(
    get,
    path = "/v1/batches/{batch_id}",
    params(("batch_id" = String, Path, description = "Wyrd batch id")),
    responses(
        (status = 200, description = "OpenAI batch object refreshed from the provider", body = GatewayBatch),
        (status = 404, description = "Unknown batch, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage or audit is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `GET /v1/batches/{batch_id}`: reads a batch from its deployment.
///
/// See [`GatewayBatches::retrieve`] for errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn get_batch(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    Path(batch_id): Path<String>,
) -> Response {
    let answer = match caller {
        Ok(caller) => {
            GatewayBatches::new(&state)
                .retrieve(&caller, &batch_id)
                .await
        }
        Err(WyrdErrorResponse(error)) => Err(error),
    };
    batch_response(answer)
}

#[utoipa::path(
    post,
    path = "/v1/batches/{batch_id}/cancel",
    params(("batch_id" = String, Path, description = "Wyrd batch id")),
    responses(
        (status = 200, description = "OpenAI batch object after cancellation was requested", body = GatewayBatch),
        (status = 404, description = "Unknown batch, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage or audit is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/batches/{batch_id}/cancel`: cancels a batch on its deployment.
///
/// See [`GatewayBatches::cancel`] for errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.invoke"))]
pub(crate) async fn cancel_batch(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    Path(batch_id): Path<String>,
) -> Response {
    let answer = match caller {
        Ok(caller) => GatewayBatches::new(&state).cancel(&caller, &batch_id).await,
        Err(WyrdErrorResponse(error)) => Err(error),
    };
    batch_response(answer)
}

/// Renders a Batches answer, or its failure as the `OpenAI` error envelope.
fn batch_response(answer: Result<BatchAnswer, WyrdError>) -> Response {
    match answer {
        Ok(answer) => relay(answer.status, answer.body),
        Err(error) => openai_error(&error),
    }
}

/// Decodes a public `OpenAI`-compatible JSON body.
///
/// # Errors
/// Returns `GatewayInvalidRequest` for field `body` when the body is not JSON.
pub(super) fn json_body(request: Result<Json<Value>, JsonRejection>) -> Result<Value, WyrdError> {
    request
        .map(|Json(value)| value)
        .map_err(|_| invalid_request(GatewayContractError::new("body", "must be a JSON object")))
}

/// Decodes a public JSON body as the shared operation request contract `T`
/// and returns it as the provider-bound body.
///
/// The body is checked by decoding `T`, so a missing or mistyped required
/// member fails before authorization or dispatch; the checked body itself is
/// forwarded, so every member `T` accepts, including flattened extensions,
/// reaches the provider exactly as sent.
///
/// # Errors
/// Returns the [`json_body`] error for a non-JSON body, and
/// `GatewayInvalidRequest` naming the missing member, or `body`, when the
/// body does not decode as `T`.
pub(super) fn typed_body<T: DeserializeOwned>(
    request: Result<Json<Value>, JsonRejection>,
) -> Result<Value, WyrdError> {
    let body = json_body(request)?;
    T::deserialize(&body).map_err(|error| {
        let message = error.to_string();
        let field = message
            .strip_prefix("missing field `")
            .and_then(|rest| rest.split('`').next())
            .unwrap_or("body");
        invalid_request(GatewayContractError::new(
            field,
            "must match the operation request contract",
        ))
    })?;
    Ok(body)
}

/// Decodes a public `multipart/form-data` media body for `route`, reading at
/// most `limit` bytes and putting uploaded files in `sink`.
///
/// # Errors
/// Returns `GatewayInvalidRequest` naming `body` when the body cannot be read
/// or is not a well-formed form, or naming a text field that is not UTF-8,
/// and `PayloadTooLarge` when the body exceeds `limit`.
async fn form_body(
    route: OpenAiMediaRoute,
    headers: &HeaderMap,
    request: Result<Body, BytesRejection>,
    limit: usize,
    sink: FileSink,
) -> Result<(Value, Option<MediaRequest>), WyrdError> {
    let body = request
        .map_err(|_| invalid_request(GatewayContractError::new("body", "could not be read")))?;
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let (body, files) = FormReader::new(body, limit)
        .decode(content_type, sink)
        .await?;
    Ok((body, Some(MediaRequest { route, files })))
}

/// Validates a decoded public body and runs it as a governed `operation`
/// call.
///
/// JSON bodies arrive already decoded against their shared contract by
/// [`typed_body`] and reach the provider as sent; form bodies arrive from
/// their form reader.
///
/// # Errors
/// Returns the decoding error of `request`, `GatewayInvalidRequest` for a
/// `model` that is not an exact projection, and every error of
/// [`GatewayInvocation::invoke`].
async fn invoke_openai(
    state: &AppState,
    caller: &Caller,
    request: Result<(Value, Option<MediaRequest>), WyrdError>,
    operation: GatewayOperation,
) -> Result<GatewayCallResponse, WyrdError> {
    let (body, media) = request?;
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| GatewayContractError::new("model", "must be <provider>/<model>"))
        .and_then(ModelRef::from_projection)
        .map_err(invalid_request)?;
    let usage_bound = usage_bound(operation, &body);
    GatewayInvocation::new(state)
        .invoke(
            caller,
            GatewayCallRequest {
                operation,
                ingress: IngressDialect::OpenAi,
                model,
                fallback: None,
                stream: body.get("stream") == Some(&Value::Bool(true)),
                body,
                media,
                batch: None,
                deployment: None,
                usage_bound,
                timeout: PUBLIC_CALL_TIMEOUT,
            },
        )
        .await
}

/// Renders a gateway-originated error as the `OpenAI` error envelope with the
/// error's HTTP status, message, stable Wyrd code, and offending field.
pub(super) fn openai_error(error: &WyrdError) -> Response {
    let status = StatusCode::from_u16(error.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let problem = error.as_problem_json();
    let body = wyrd_gateway::openai_error_body(
        status.as_u16(),
        problem["detail"].as_str().unwrap_or_else(|| error.title()),
        Some(error.code()),
        problem.pointer("/details/field").and_then(Value::as_str),
    );
    (status, Json(body)).into_response()
}

/// Largest token usage an `operation` request can incur, or `None` when it is
/// unbounded.
///
/// Input tokens are bounded by the serialized body length, since no token is
/// shorter than one byte. Chat Completions bound output by
/// `max_completion_tokens` (or legacy `max_tokens`) times the choice count
/// `n`, Responses by `max_output_tokens`, and a request without that maximum
/// is unbounded; Embeddings produce no output tokens. Other operations have no
/// token bound here.
fn usage_bound(operation: GatewayOperation, body: &Value) -> Option<Vec<GatewayUsageAmount>> {
    let field = |name: &str| body.get(name).and_then(Value::as_u64);
    let output = match operation {
        GatewayOperation::ChatCompletions => Some(
            field("max_completion_tokens")
                .or_else(|| field("max_tokens"))?
                .checked_mul(field("n").unwrap_or(1))?,
        ),
        GatewayOperation::Responses => Some(field("max_output_tokens")?),
        GatewayOperation::Embeddings => None,
        GatewayOperation::Images | GatewayOperation::Audio | GatewayOperation::Batches => {
            return None;
        }
    };
    token_bound(body, output)
}

/// Token usage bound of a request `body` whose output is at most `output`
/// tokens, or `None` when a quantity cannot be represented.
///
/// Input tokens are bounded by the serialized body length, since no token is
/// shorter than one byte; `output` of `None` adds no output dimension.
pub(super) fn token_bound(body: &Value, output: Option<u64>) -> Option<Vec<GatewayUsageAmount>> {
    let input = u64::try_from(body.to_string().len()).ok()?;
    let amount = |dimension: &str, quantity: u64| {
        Some(GatewayUsageAmount {
            dimension: dimension.to_owned(),
            unit: "tokens".to_owned(),
            quantity: GatewayDecimal::new(&quantity.to_string()).ok()?,
        })
    };
    let mut bound = vec![amount("input_tokens", input)?];
    if let Some(output) = output {
        bound.push(amount("output_tokens", output)?);
    }
    Some(bound)
}

#[cfg(test)]
mod usage_bound_tests {
    use serde_json::json;
    use wyrd_spec::gateway::GatewayOperation;

    use super::usage_bound;

    /// Chat output is bounded by the maximum times the choice count, Responses
    /// output by `max_output_tokens`, and input by the body length; a chat or
    /// Responses request without a maximum is unbounded, and embeddings bound
    /// only input.
    ///
    /// # Panics
    ///
    /// Panics when a bound differs from the expected amounts.
    #[test]
    fn usage_bound_requires_an_output_maximum_where_output_exists() {
        let length = |body: &serde_json::Value| body.to_string().len().to_string();
        let chat = json!({"model": "acme/a", "max_tokens": 50, "n": 2});
        let bound = usage_bound(GatewayOperation::ChatCompletions, &chat).expect("bounded");
        assert_eq!(bound[0].quantity.as_str(), length(&chat));
        assert_eq!(bound[1].quantity.as_str(), "100");
        let responses = json!({"model": "acme/a", "max_output_tokens": 7});
        let bound = usage_bound(GatewayOperation::Responses, &responses).expect("bounded");
        assert_eq!(bound[1].quantity.as_str(), "7");
        let embeddings = json!({"model": "acme/a", "input": ["a", "b"]});
        let bound = usage_bound(GatewayOperation::Embeddings, &embeddings).expect("bounded");
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].quantity.as_str(), length(&embeddings));
        for operation in [
            GatewayOperation::ChatCompletions,
            GatewayOperation::Responses,
        ] {
            assert!(usage_bound(operation, &json!({"model": "acme/a"})).is_none());
        }
    }
}
