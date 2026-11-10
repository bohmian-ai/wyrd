//! HTTP projection of tenant gateway administration.
//!
//! Handlers parse the path name, then delegate to [`GatewayAdministration`],
//! which owns authorization, audit, validation, and persistence.

use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::rejection::{BytesRejection, JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use skald_spec::wire::borrowed::Borrowed;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use vala_bifrost_redux::catalog::{BifrostCatalogError, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_gateway::{IngressDialect, MediaRequest, OpenAiMediaRoute, RequestJson, ResponseBody};
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
    ProviderDeploymentsApplied, ProviderDeploymentsApply,
};
use wyrd_spec::ids::{ProviderCredentialName, ProviderDeploymentName};
use wyrd_storage::StorageError;

use super::batches::{BatchAnswer, GatewayBatches};
use super::capture::object_path;
use super::ingress::{
    CARD_UID_HEADER_DOC, FALLBACK_HEADER_DOC, PROMPT_PROVIDER_HEADER_DOC, RUN_HEADER_DOC,
    requested_context, requested_fallback, requested_prompt_provider,
};
use super::invocation::{
    GatewayCallRequest, GatewayCallResponse, GatewayInvocation, invalid_request,
};
use super::multipart::{FileSink, FormReader};
use super::service::{GatewayAdministration, invalid, unavailable};
use super::telemetry::IngressClock;
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
        .routes(routes!(list_deployments, apply_deployments))
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
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
/// scoped to this tenant's registered `bifrost.gateway.calls`, and each decision
/// is staged on the process audit outbox before the bytes are read. The table
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
/// `ServiceUnavailable` for catalog or storage failure and for stored
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
    )?;
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
    )?;
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
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
/// would orphan a deployment, and authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/provider-credentials/{name}`.
///
/// Reads one redacted credential view.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, `GatewayResourceNotFound`
/// when absent, and authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/provider-credentials`.
///
/// Lists the tenant's redacted credential views ordered by name.
///
/// # Errors
/// Returns authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `POST /v1/admin/gateway/provider-credentials/{name}/revoke`.
///
/// Terminally revokes one credential; repeating returns the same revocation.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, `GatewayResourceNotFound`
/// when absent, and authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `DELETE /v1/admin/gateway/provider-credentials/{name}`.
///
/// Deletes an unreferenced credential; an absent name succeeds with 204.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, `GatewayResourceConflict`
/// while a deployment references it, and authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `PUT /v1/admin/gateway/provider-deployments/{name}`.
///
/// Creates or replaces one deployment; only newly admitted calls observe it.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, document, or credential
/// reference, and authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/provider-deployments/{name}`.
///
/// Reads one deployment.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, `GatewayResourceNotFound`
/// when absent, and authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/provider-deployments`.
///
/// Lists the tenant's deployments ordered by name.
///
/// # Errors
/// Returns authentication, permission, or storage errors.
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
    post,
    path = "/admin/gateway/provider-deployments",
    request_body = ProviderDeploymentsApply,
    responses(
        (status = 200, body = ProviderDeploymentsApplied),
        (status = 400, description = "Invalid document, credential source, or credential reference; nothing is applied (WYRD_GATEWAY_400_INVALID_CONFIGURATION)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 409, description = "A submitted credential is revoked or its provider change would orphan a deployment; nothing is applied (WYRD_GATEWAY_409_RESOURCE_CONFLICT)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 403, description = "Gateway permission required (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 503, description = "Credential protection, administration store, or token verification unavailable; a refusal before commit applies nothing, but a failed commit acknowledgement can leave the document applied, so list the resources or reapply the same document (WYRD_SERVER_503_SERVICE_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `POST /v1/admin/gateway/provider-deployments`.
///
/// Creates or replaces credentials and the deployments using them in one
/// tenant transaction; answers with redacted credential views. The answer is
/// sent after commit, so a caller that loses it cannot infer noncommit;
/// reapplying the same named document converges.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid document, source, or
/// reference, `GatewayResourceConflict` for a revoked credential, and
/// authentication, permission, sealing, or storage errors.
#[tracing::instrument(skip_all, fields(operation = "gateway.provider_deployments.apply"))]
pub(crate) async fn apply_deployments(
    State(state): State<AppState>,
    caller: Caller,
    apply: Result<Json<ProviderDeploymentsApply>, JsonRejection>,
) -> GatewayResult<Json<ProviderDeploymentsApplied>> {
    let apply = body(apply)?;
    Ok(Json(
        GatewayAdministration::new(&state)
            .apply_deployments(&caller, apply)
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `DELETE /v1/admin/gateway/provider-deployments/{name}`.
///
/// Deletes one deployment; an absent name succeeds with 204.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid name, and authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `PUT /v1/admin/gateway/fallback-policy`.
///
/// Atomically replaces the tenant fallback policy for newly admitted calls.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid body or policy, and
/// authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/fallback-policy`.
///
/// Reads the tenant fallback policy, or the empty default.
///
/// # Errors
/// Returns authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `DELETE /v1/admin/gateway/fallback-policy`.
///
/// Restores the empty fallback policy; repeating succeeds.
///
/// # Errors
/// Returns authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `PUT /v1/admin/gateway/governance-policy`.
///
/// Atomically replaces governance, retaining omitted pricing versions inactive.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid body or policy or a changed
/// pricing version, and authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/governance-policy`.
///
/// Reads the tenant governance policy with its pricing history.
///
/// # Errors
/// Returns authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `DELETE /v1/admin/gateway/governance-policy`.
///
/// Clears limits and budgets and retires pricing; repeating succeeds.
///
/// # Errors
/// Returns authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `PUT /v1/admin/gateway/capture-policy`.
///
/// Replaces the capture policy, bumping its version only on a content change.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for an invalid body or mode/field combination,
/// and authentication, permission, or storage errors.
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
        (status = "default", description = "Other gateway refusal (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem, content_type = "application/problem+json")
    ),
    tag = "Gateway"
)]
/// `GET /v1/admin/gateway/capture-policy`.
///
/// Reads the capture policy, or the disabled version-1 default.
///
/// # Errors
/// Returns authentication, permission, or storage errors.
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
    params(
        ("wyrd-gateway-fallback" = Option<String>, Header, description = FALLBACK_HEADER_DOC),
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC),
        ("wyrd-prompt-provider" = Option<String>, Header, description = PROMPT_PROVIDER_HEADER_DOC)
    ),
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
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn chat_completions(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    headers: HeaderMap,
    request: Result<JsonBody, WyrdErrorResponse>,
) -> Response {
    openai_call(
        &state,
        caller,
        clock.map(|Extension(clock)| clock),
        typed_body::<GatewayChatCompletionsRequest>(request).map(|body| (body, None)),
        GatewayOperation::ChatCompletions,
        Some(&headers),
    )
    .await
}

#[utoipa::path(
    post,
    path = "/v1/responses",
    params(
        ("wyrd-gateway-fallback" = Option<String>, Header, description = FALLBACK_HEADER_DOC),
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC),
        ("wyrd-prompt-provider" = Option<String>, Header, description = PROMPT_PROVIDER_HEADER_DOC)
    ),
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
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn responses(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    headers: HeaderMap,
    request: Result<JsonBody, WyrdErrorResponse>,
) -> Response {
    openai_call(
        &state,
        caller,
        clock.map(|Extension(clock)| clock),
        typed_body::<GatewayResponsesRequest>(request).map(|body| (body, None)),
        GatewayOperation::Responses,
        Some(&headers),
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
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn embeddings(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    request: Result<JsonBody, WyrdErrorResponse>,
) -> Response {
    openai_call(
        &state,
        caller,
        clock.map(|Extension(clock)| clock),
        typed_body::<GatewayEmbeddingsRequest>(request).map(|body| (body, None)),
        GatewayOperation::Embeddings,
        None,
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
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn image_generations(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    request: Result<JsonBody, WyrdErrorResponse>,
) -> Response {
    json_media(
        &state,
        caller,
        clock.map(|Extension(clock)| clock),
        request,
        OpenAiMediaRoute::ImageGenerations,
    )
    .await
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
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn image_edits(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    headers: HeaderMap,
    request: Result<Bytes, BytesRejection>,
) -> Response {
    let request = form_body(
        OpenAiMediaRoute::ImageEdits,
        &headers,
        request.map(Body::from),
        state.settings.limits.body_bytes,
        FileSink::Memory,
    )
    .await;
    openai_call(
        &state,
        caller,
        clock.map(|Extension(clock)| clock),
        request,
        GatewayOperation::Images,
        None,
    )
    .await
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
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn image_variations(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    headers: HeaderMap,
    request: Result<Bytes, BytesRejection>,
) -> Response {
    let request = form_body(
        OpenAiMediaRoute::ImageVariations,
        &headers,
        request.map(Body::from),
        state.settings.limits.body_bytes,
        FileSink::Memory,
    )
    .await;
    openai_call(
        &state,
        caller,
        clock.map(|Extension(clock)| clock),
        request,
        GatewayOperation::Images,
        None,
    )
    .await
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
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn audio_speech(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    request: Result<JsonBody, WyrdErrorResponse>,
) -> Response {
    json_media(
        &state,
        caller,
        clock.map(|Extension(clock)| clock),
        request,
        OpenAiMediaRoute::AudioSpeech,
    )
    .await
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
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn audio_transcriptions(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    audio_form(
        &state,
        caller,
        clock.map(|Extension(clock)| clock),
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
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn audio_translations(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<Extension<IngressClock>>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    audio_form(
        &state,
        caller,
        clock.map(|Extension(clock)| clock),
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
    clock: Option<IngressClock>,
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
                state.settings.limits.audio_upload_bytes,
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
    openai_call(state, caller, clock, request, GatewayOperation::Audio, None).await
}

/// Runs a JSON media `route` of its operation family, whose body must decode
/// as the route's shared request contract.
///
/// See [`openai_call`] for errors and buffering.
async fn json_media(
    state: &AppState,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<IngressClock>,
    request: Result<JsonBody, WyrdErrorResponse>,
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
        clock,
        body.map(|body| (body, Some(media))),
        operation,
        None,
    )
    .await
}

#[utoipa::path(
    get,
    path = "/v1/models",
    responses(
        (status = 200, description = "OpenAI model list of the exact `<provider>/<model>` projections configured for the tenant that the caller may invoke", body = GatewayModelList),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or configuration is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
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
                    id: model.to_string(),
                    object: "model".to_owned(),
                    created: 0,
                    owned_by: model.deployment.as_str().to_owned(),
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
///
/// `headers` are the request headers of a route that accepts the
/// `wyrd-gateway-fallback` override, the `wyrd-run-id`/`wyrd-card-uid`
/// attribution, and the `wyrd-prompt-provider` Agent binding; routes that do
/// not pass `None`, keeping tenant fallback policy and recording an
/// uncorrelated direct call.
async fn openai_call(
    state: &AppState,
    caller: Result<Caller, WyrdErrorResponse>,
    clock: Option<IngressClock>,
    request: Result<(RequestJson, Option<MediaRequest>), WyrdError>,
    operation: GatewayOperation,
    headers: Option<&HeaderMap>,
) -> Response {
    let result = match caller {
        Ok(caller) => invoke_openai(state, &caller, clock, request, operation, headers).await,
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
            raw.into_string(),
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
    params(
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC)
    ),
    request_body(content = GatewayFileUploadForm, content_type = "multipart/form-data", description = "OpenAI-compatible file upload form: `purpose` `batch` and exactly one batch JSONL `file` whose request lines all target one endpoint and one exact `<provider>/<model>` projection"),
    responses(
        (status = 200, description = "OpenAI file object with a Wyrd file id", body = GatewayFile),
        (status = 400, description = "Malformed form or batch input file, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 404, description = "No authorized deployment serves Batches for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 413, description = "Form exceeds the request body limit (WYRD_SPEC_413_PAYLOAD_TOO_LARGE)", body = WyrdProblem, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
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
    let answer = match (caller, requested_context(&headers), request) {
        (Err(WyrdErrorResponse(error)), _, _) | (Ok(_), Err(error), _) => Err(error),
        (Ok(_), Ok(_), Err(_)) => Err(invalid_request(GatewayContractError::new(
            "body",
            "could not be read",
        ))),
        (Ok(caller), Ok(context), Ok(bytes)) => {
            let content_type = headers
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok());
            GatewayBatches::new(&state, context)
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
        (status = 503, description = "The server is draining, or storage is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
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
        Ok(caller) => {
            GatewayBatches::new(&state, None)
                .file(&caller, &file_id)
                .await
        }
        Err(WyrdErrorResponse(error)) => Err(error),
    };
    batch_response(answer)
}

#[utoipa::path(
    get,
    path = "/v1/files/{file_id}/content",
    params(
        ("file_id" = String, Path, description = "Wyrd id of an uploaded file, or of a batch output or error file"),
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC)
    ),
    responses(
        (status = 200, description = "File bytes in the provider's content type", content(
            (String = "application/octet-stream")
        )),
        (status = 404, description = "Unknown file, or a batch file not yet observed, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 502, description = "No provider attempt completed or the content exceeded its bound, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `GET /v1/files/{file_id}/content`: relays a file's bounded content from
/// the deployment holding it.
///
/// See [`GatewayBatches::file_content`] for errors.
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn file_content(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: HeaderMap,
    Path(file_id): Path<String>,
) -> Response {
    let answer = match (caller, requested_context(&headers)) {
        (Err(WyrdErrorResponse(error)), _) | (Ok(_), Err(error)) => Err(error),
        (Ok(caller), Ok(context)) => {
            GatewayBatches::new(&state, context)
                .file_content(&caller, &file_id)
                .await
        }
    };
    batch_response(answer)
}

#[utoipa::path(
    delete,
    path = "/v1/files/{file_id}",
    params(
        ("file_id" = String, Path, description = "Wyrd id of an uploaded batch input file"),
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC)
    ),
    responses(
        (status = 200, description = "OpenAI file deletion object", body = GatewayFileDeleted),
        (status = 404, description = "Unknown file, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `DELETE /v1/files/{file_id}`: deletes an uploaded file from its
/// deployment and the tenant.
///
/// See [`GatewayBatches::delete_file`] for errors.
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn delete_file(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: HeaderMap,
    Path(file_id): Path<String>,
) -> Response {
    let answer = match (caller, requested_context(&headers)) {
        (Err(WyrdErrorResponse(error)), _) | (Ok(_), Err(error)) => Err(error),
        (Ok(caller), Ok(context)) => {
            GatewayBatches::new(&state, context)
                .delete_file(&caller, &file_id)
                .await
        }
    };
    batch_response(answer)
}

#[utoipa::path(
    post,
    path = "/v1/batches",
    params(
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC)
    ),
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
        (status = 503, description = "The server is draining, or storage is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/batches`: creates a batch at most once per canonical request.
///
/// See [`GatewayBatches::create`] for errors.
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn create_batch(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: HeaderMap,
    request: Result<JsonBody, WyrdErrorResponse>,
) -> Response {
    let answer = match (
        caller,
        requested_context(&headers),
        typed_body::<GatewayBatchCreateRequest>(request),
    ) {
        (Err(WyrdErrorResponse(error)), _, _)
        | (Ok(_), Err(error), _)
        | (Ok(_), Ok(_), Err(error)) => Err(error),
        (Ok(caller), Ok(context), Ok(body)) => {
            GatewayBatches::new(&state, context)
                .create(&caller, body.into_tree())
                .await
        }
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
        (status = 503, description = "The server is draining, or storage is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
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
            GatewayBatches::new(&state, None)
                .list(&caller, query.after.as_deref(), query.limit)
                .await
        }
    };
    batch_response(answer)
}

#[utoipa::path(
    get,
    path = "/v1/batches/{batch_id}",
    params(
        ("batch_id" = String, Path, description = "Wyrd batch id"),
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC)
    ),
    responses(
        (status = 200, description = "OpenAI batch object refreshed from the provider", body = GatewayBatch),
        (status = 404, description = "Unknown batch, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `GET /v1/batches/{batch_id}`: reads a batch from its deployment.
///
/// See [`GatewayBatches::retrieve`] for errors.
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn get_batch(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: HeaderMap,
    Path(batch_id): Path<String>,
) -> Response {
    let answer = match (caller, requested_context(&headers)) {
        (Err(WyrdErrorResponse(error)), _) | (Ok(_), Err(error)) => Err(error),
        (Ok(caller), Ok(context)) => {
            GatewayBatches::new(&state, context)
                .retrieve(&caller, &batch_id)
                .await
        }
    };
    batch_response(answer)
}

#[utoipa::path(
    post,
    path = "/v1/batches/{batch_id}/cancel",
    params(
        ("batch_id" = String, Path, description = "Wyrd batch id"),
        ("wyrd-run-id" = Option<String>, Header, description = RUN_HEADER_DOC),
        ("wyrd-card-uid" = Option<String>, Header, description = CARD_UID_HEADER_DOC)
    ),
    responses(
        (status = 200, description = "OpenAI batch object after cancellation was requested", body = GatewayBatch),
        (status = 404, description = "Unknown batch, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 401, description = "Authentication required, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 403, description = "Invoke permission required for the model, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = 503, description = "The server is draining, or storage is unavailable, as an OpenAI error envelope", body = OpenAiErrorEnvelope),
        (status = "default", description = "Other gateway refusal as an OpenAI error envelope, or the refusing provider's status with its error body", body = OpenAiErrorEnvelope)
    ),
    tag = "Gateway"
)]
/// `POST /v1/batches/{batch_id}/cancel`: cancels a batch on its deployment.
///
/// See [`GatewayBatches::cancel`] for errors.
#[tracing::instrument(
    name = "gateway.invoke",
    skip_all,
    fields(operation = "gateway.invoke")
)]
pub(crate) async fn cancel_batch(
    State(state): State<AppState>,
    caller: Result<Caller, WyrdErrorResponse>,
    headers: HeaderMap,
    Path(batch_id): Path<String>,
) -> Response {
    let answer = match (caller, requested_context(&headers)) {
        (Err(WyrdErrorResponse(error)), _) | (Ok(_), Err(error)) => Err(error),
        (Ok(caller), Ok(context)) => {
            GatewayBatches::new(&state, context)
                .cancel(&caller, &batch_id)
                .await
        }
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

/// A public JSON request body, read as [`Json`] reads one: a JSON media type,
/// the router's body limit, and one JSON value without trailing input.
///
/// The body is kept as [`RequestJson`], so compact text reaches validation
/// and the provider without being decoded into an owned tree.
pub(crate) struct JsonBody(RequestJson);

impl<S: Send + Sync> axum::extract::FromRequest<S> for JsonBody {
    type Rejection = WyrdErrorResponse;

    /// Reads the request's JSON body.
    ///
    /// # Errors
    /// Returns `GatewayInvalidRequest` for field `body` when the content type
    /// is not JSON or the body cannot be read or is not JSON.
    async fn from_request(
        request: axum::extract::Request,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let refused = || {
            WyrdErrorResponse(invalid_request(GatewayContractError::new(
                "body",
                "must be a JSON object",
            )))
        };
        if !json_content_type(request.headers()).await {
            return Err(refused());
        }
        let text = Bytes::from_request(request, state)
            .await
            .map_err(|_| refused())?;
        RequestJson::parse(text).map(Self).map_err(|_| refused())
    }
}

#[cfg(test)]
impl From<Value> for JsonBody {
    /// The body as it arrives as compact text, which an object keeps
    /// undecoded.
    fn from(body: Value) -> Self {
        let text = serde_json::to_vec(&body).unwrap_or_default();
        Self(RequestJson::parse(text.into()).unwrap_or_else(|_| RequestJson::from(body)))
    }
}

/// Whether `headers` name a JSON media type, judged by [`Json`] itself from
/// a probe carrying only the content type.
async fn json_content_type(headers: &HeaderMap) -> bool {
    let mut probe = axum::extract::Request::new(Body::empty());
    if let Some(content_type) = headers.get(CONTENT_TYPE) {
        probe
            .headers_mut()
            .insert(CONTENT_TYPE, content_type.clone());
    }
    !matches!(
        <Json<serde::de::IgnoredAny> as axum::extract::FromRequest<()>>::from_request(probe, &())
            .await,
        Err(JsonRejection::MissingJsonContentType(_))
    )
}
/// Takes a public `OpenAI`-compatible JSON body.
///
/// # Errors
/// Returns `GatewayInvalidRequest` for field `body` when the body is not JSON.
pub(super) fn json_body(
    request: Result<JsonBody, WyrdErrorResponse>,
) -> Result<RequestJson, WyrdError> {
    request
        .map(|JsonBody(body)| body)
        .map_err(|WyrdErrorResponse(error)| error)
}

/// Validates a public JSON body against the shared operation request
/// contract `T` and returns it as the provider-bound body.
///
/// The body is checked by decoding `T`'s borrowed projection, which applies
/// `T`'s rules while borrowing its text, so a missing or mistyped required
/// member fails before authorization or dispatch without copying the payload;
/// the checked body itself is forwarded, so every member `T` accepts,
/// including flattened extensions, reaches the provider exactly as sent. Kept
/// compact text is read in place through [`RequestJson::view`].
///
/// # Errors
/// Returns the [`json_body`] error for a non-JSON body, and
/// `GatewayInvalidRequest` naming the missing member, or `body`, when the
/// body does not decode as `T`.
pub(super) fn typed_body<T: for<'a> Borrowed<'a>>(
    request: Result<JsonBody, WyrdErrorResponse>,
) -> Result<RequestJson, WyrdError> {
    let body = json_body(request)?;
    body.view::<<T as Borrowed<'_>>::View>().map_err(|error| {
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
) -> Result<(RequestJson, Option<MediaRequest>), WyrdError> {
    let body = request
        .map_err(|_| invalid_request(GatewayContractError::new("body", "could not be read")))?;
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let (body, files) = FormReader::new(body, limit)
        .decode(content_type, sink)
        .await?;
    Ok((RequestJson::from(body), Some(MediaRequest { route, files })))
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
/// `model` that is not an exact projection, every error of
/// [`requested_fallback`], [`requested_context`], and
/// [`requested_prompt_provider`] for `headers`, and every
/// error of
/// [`GatewayInvocation::invoke`].
async fn invoke_openai(
    state: &AppState,
    caller: &Caller,
    clock: Option<IngressClock>,
    request: Result<(RequestJson, Option<MediaRequest>), WyrdError>,
    operation: GatewayOperation,
    headers: Option<&HeaderMap>,
) -> Result<GatewayCallResponse, WyrdError> {
    let (body, media) = request?;
    let model = body
        .member("model")
        .as_deref()
        .and_then(Value::as_str)
        .ok_or_else(|| GatewayContractError::new("model", "must be <provider>/<model>"))
        .and_then(ModelRef::from_projection)
        .map_err(invalid_request)?;
    let (fallback, subject, prompt_provider) = match headers {
        Some(headers) => (
            requested_fallback(headers, &model)?,
            requested_context(headers)?,
            requested_prompt_provider(headers)?,
        ),
        None => (None, None, None),
    };
    let usage_bound = usage_bound(operation, &body);
    GatewayInvocation::new(state)
        .parsed(clock, operation)
        .invoke(
            caller,
            GatewayCallRequest {
                operation,
                ingress: IngressDialect::OpenAi,
                model,
                fallback,
                stream: body.member("stream").as_deref() == Some(&Value::Bool(true)),
                body,
                media,
                batch: None,
                deployment: None,
                usage_bound,
                timeout: PUBLIC_CALL_TIMEOUT,
                subject,
                prompt_provider,
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
/// token bound here, including Jev `systemone`, whose answers no request
/// member bounds.
pub(super) fn usage_bound(
    operation: GatewayOperation,
    body: &RequestJson,
) -> Option<Vec<GatewayUsageAmount>> {
    let field = |name: &str| body.member(name).and_then(|member| member.as_u64());
    let output = match operation {
        GatewayOperation::ChatCompletions => Some(
            field("max_completion_tokens")
                .or_else(|| field("max_tokens"))?
                .checked_mul(field("n").unwrap_or(1))?,
        ),
        GatewayOperation::Responses => Some(field("max_output_tokens")?),
        GatewayOperation::Embeddings => None,
        GatewayOperation::Images
        | GatewayOperation::Audio
        | GatewayOperation::Batches
        | GatewayOperation::SystemOne => {
            return None;
        }
    };
    token_bound(body, output)
}

/// Token usage bound of a request `body` whose output is at most `output`
/// tokens, or `None` when a quantity cannot be represented.
///
/// Input tokens are bounded by the compact serialized body length, since no
/// token is shorter than one byte. Kept compact text is that serialization,
/// so its length is read directly; a decoded body's length is counted as it
/// serializes, without keeping its text. `output` of `None` adds no output
/// dimension.
pub(super) fn token_bound(
    body: &RequestJson,
    output: Option<u64>,
) -> Option<Vec<GatewayUsageAmount>> {
    let length = if let Some(text) = body.text() {
        text.len()
    } else {
        let mut length = SerializedLength(0);
        serde_json::to_writer(&mut length, body.tree()).ok()?;
        length.0
    };
    let input = u64::try_from(length).ok()?;
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

/// Byte count of everything written to it; the written bytes are dropped.
pub(super) struct SerializedLength(pub(super) usize);

impl std::io::Write for SerializedLength {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod usage_bound_tests {
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_gateway::{
        AttemptResult, BatchAction, BatchInput, BuiltinEndpoints, CallPlan, GatewayTenantSnapshot,
        HttpProviderDispatch, IngressDialect, MediaRequest, OpenAiMediaRoute, ProviderAttempt,
        ProviderDispatch, ProviderSecret, RequestJson, UploadContent, UploadFile,
    };
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::gateway::native::{
        GatewayAnthropicMessagesRequest, GatewayGeminiGenerateContentRequest, GatewayJevRequest,
    };
    use wyrd_spec::gateway::openai::{
        GatewayBatchCreateRequest, GatewayChatCompletionsRequest, GatewayEmbeddingsRequest,
        GatewayImageGenerationRequest, GatewayResponsesRequest, GatewaySpeechRequest,
    };
    use wyrd_spec::gateway::{
        GatewayCallId, GatewayGovernancePolicy, GatewayOperation, GatewayUsageAmount, ModelRef,
        ProviderDeployment,
    };

    use super::super::ingress::{anthropic_usage_bound, google_usage_bound};
    use super::{Body, CONTENT_TYPE, HeaderValue, Json, JsonBody, typed_body, usage_bound};
    use crate::http::error::WyrdErrorResponse;
    use crate::test_support::CountingAllocator;

    /// `body` as it arrives: its compact text, which an object keeps
    /// undecoded.
    ///
    /// # Panics
    ///
    /// Panics when `body` does not serialize.
    fn arrived(body: &Value) -> RequestJson {
        RequestJson::parse(serde_json::to_vec(body).expect("json").into()).expect("decodes")
    }

    /// The usage bound of `body` as it arrives, asserting a decoded body is
    /// bounded alike.
    ///
    /// # Panics
    ///
    /// Panics when the two bounds differ.
    fn bounded(operation: GatewayOperation, body: &Value) -> Option<Vec<GatewayUsageAmount>> {
        let kept = usage_bound(operation, &arrived(body));
        let decoded = usage_bound(operation, &RequestJson::from(body.clone()));
        assert_eq!(
            format!("{kept:?}"),
            format!("{decoded:?}"),
            "kept and decoded bounds of {body}"
        );
        kept
    }

    /// Chat output is bounded by the maximum times the choice count, Responses
    /// output by `max_output_tokens`, and input by the body length; a chat or
    /// Responses request without a maximum is unbounded, and embeddings bound
    /// only input. The input bound is the compact JSON length, escapes,
    /// Unicode and number forms included, counted without serializing the
    /// body.
    ///
    /// # Panics
    ///
    /// Panics when a bound differs from the expected amounts or counting
    /// allocates request-sized text.
    #[test]
    fn usage_bound_requires_an_output_maximum_where_output_exists() {
        let length = |body: &serde_json::Value| body.to_string().len().to_string();
        let chat = json!({"model": "acme/a", "max_tokens": 50, "n": 2});
        let bound = bounded(GatewayOperation::ChatCompletions, &chat).expect("bounded");
        assert_eq!(bound[0].quantity.as_str(), length(&chat));
        assert_eq!(bound[1].quantity.as_str(), "100");
        let responses = json!({"model": "acme/a", "max_output_tokens": 7});
        let bound = bounded(GatewayOperation::Responses, &responses).expect("bounded");
        assert_eq!(bound[1].quantity.as_str(), "7");
        let embeddings = json!({"model": "acme/a", "input": ["a", "b"]});
        let bound = bounded(GatewayOperation::Embeddings, &embeddings).expect("bounded");
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].quantity.as_str(), length(&embeddings));
        for operation in [
            GatewayOperation::ChatCompletions,
            GatewayOperation::Responses,
        ] {
            assert!(bounded(operation, &json!({"model": "acme/a"})).is_none());
        }
        let overflowing = json!({"model": "acme/a", "max_tokens": u64::MAX, "n": 2});
        assert!(bounded(GatewayOperation::ChatCompletions, &overflowing).is_none());

        for body in [
            json!({"input": "quote \" slash \\ tab \t nul \u{0} bell \u{7}"}),
            json!({"input": "é ☃ 🦀 \u{2028}", "ключ": ["ü"]}),
            json!({"input": [[1, {"a": {"b": [true, false, null]}}], {}, []]}),
            json!({"input": [0, -1, u64::MAX, i64::MIN, 1.5, -0.0, 1e300, 2.5e-8]}),
        ] {
            let bound = bounded(GatewayOperation::Embeddings, &body).expect("bounded");
            assert_eq!(bound[0].quantity.as_str(), length(&body), "{body}");
        }

        let long = "x".repeat(1 << 20);
        let large = json!({"model": "acme/a", "messages": [{"content": long}], "max_tokens": 1});
        for large in [arrived(&large), RequestJson::from(large.clone())] {
            let (bound, allocated) = CountingAllocator::measure(|| {
                usage_bound(GatewayOperation::ChatCompletions, &large).expect("bounded")
            });
            assert_eq!(bound[0].quantity.as_str(), length(large.tree()));
            assert!(
                allocated < 4096,
                "counting serializes no request text: {allocated} bytes allocated"
            );
        }
    }

    /// The error projection of an owned decode of `body` as `T`: the missing
    /// member, else `body`.
    fn owned<T: DeserializeOwned>(body: &Value) -> Result<(), String> {
        T::deserialize(body).map(drop).map_err(|error| {
            let message = error.to_string();
            message
                .strip_prefix("missing field `")
                .and_then(|rest| rest.split('`').next())
                .unwrap_or("body")
                .to_owned()
        })
    }

    /// One operation's request validation, as the routes call it.
    type Validate = fn(Result<JsonBody, WyrdErrorResponse>) -> Result<RequestJson, WyrdError>;

    /// One operation's owned reference decode.
    type Owned = fn(&Value) -> Result<(), String>;

    /// One family's usage bound of a request body.
    type Bound = fn(&RequestJson) -> Option<Vec<GatewayUsageAmount>>;

    /// Validates `body` with `validate` as it arrives, asserting a decoded
    /// body is judged alike and the checked body is returned unchanged, and
    /// projects a refusal to its `field`.
    fn checked(validate: Validate, body: &Value) -> Result<(), String> {
        let decoded = validate(Ok(JsonBody(RequestJson::from(body.clone()))));
        match validate(Ok(JsonBody(arrived(body)))) {
            Ok(returned) => {
                assert_eq!(
                    returned.tree(),
                    body,
                    "validation returns the body unchanged"
                );
                assert_eq!(decoded.expect("decoded body valid").tree(), body);
                Ok(())
            }
            Err(WyrdError::GatewayInvalidRequest { details, .. }) => {
                let field = details["field"].as_str().expect("field").to_owned();
                assert!(
                    matches!(decoded, Err(WyrdError::GatewayInvalidRequest { details, .. }) if details["field"] == field.as_str()),
                    "decoded body refused alike: {body}"
                );
                Err(field)
            }
            Err(error) => panic!("unexpected refusal {error}"),
        }
    }

    /// Every typed ingress operation's owned decoder and its validation.
    fn decoders() -> [(&'static str, Owned, Validate); 6] {
        [
            (
                "chat",
                owned::<GatewayChatCompletionsRequest>,
                typed_body::<GatewayChatCompletionsRequest>,
            ),
            (
                "responses",
                owned::<GatewayResponsesRequest>,
                typed_body::<GatewayResponsesRequest>,
            ),
            (
                "embeddings",
                owned::<GatewayEmbeddingsRequest>,
                typed_body::<GatewayEmbeddingsRequest>,
            ),
            (
                "speech",
                owned::<GatewaySpeechRequest>,
                typed_body::<GatewaySpeechRequest>,
            ),
            (
                "image",
                owned::<GatewayImageGenerationRequest>,
                typed_body::<GatewayImageGenerationRequest>,
            ),
            (
                "batch",
                owned::<GatewayBatchCreateRequest>,
                typed_body::<GatewayBatchCreateRequest>,
            ),
        ]
    }

    /// A valid body of `operation` whose message text is `text` and whose
    /// extension member, where the contract has one, is `extension`.
    fn sized(operation: &str, text: &str, extension: &str) -> Value {
        match operation {
            "chat" => json!({
                "model": "acme/a",
                "messages": [{"role": "user", "content": text}],
                "x_extension": {"note": extension},
            }),
            "responses" => json!({
                "model": "acme/a",
                "input": [{"role": "user", "content": text}],
                "x_extension": {"note": extension},
            }),
            "embeddings" => json!({"model": "acme/a", "input": [text, extension]}),
            "speech" => json!({
                "model": "acme/a",
                "input": text,
                "voice": "alloy",
                "x_extension": [extension],
            }),
            "image" => json!({"model": "acme/a", "prompt": text, "x_extension": extension}),
            "batch" => json!({
                "input_file_id": "file-1",
                "endpoint": "/v1/chat/completions",
                "completion_window": "24h",
                "metadata": {"note": text},
                "x_extension": extension,
            }),
            other => unreachable!("no {other} operation"),
        }
    }

    /// A native `OpenAI` attempt of `operation` with `body` and its media
    /// request or batch action.
    struct Native<'a> {
        /// Gateway operation of the attempt.
        operation: GatewayOperation,
        /// The caller's body.
        body: &'a RequestJson,
        /// Media route and files of an Images or Audio attempt.
        media: Option<&'a MediaRequest>,
        /// Batches lifecycle action of a Batches attempt.
        batch: Option<&'a BatchAction>,
    }

    /// Bytes this thread allocates dispatching `native` to the deployment of
    /// `dispatch`, a provider that answers every request.
    ///
    /// # Panics
    ///
    /// Panics when the attempt does not complete.
    fn dispatched(
        dispatch: &HttpProviderDispatch,
        deployment: &ProviderDeployment,
        native: &Native<'_>,
    ) -> usize {
        let secret = ProviderSecret::new("key".to_owned().into());
        let cancel = tokio_util::sync::CancellationToken::new();
        let model = &deployment.models.as_ref().expect("models")[0];
        let (result, bytes) = CountingAllocator::measure(|| {
            wyrd_runtime::runtime().block_on(dispatch.dispatch(ProviderAttempt {
                call_id: GatewayCallId::from_uuid(uuid::Uuid::from_u128(0xabc)),
                ordinal: std::num::NonZeroU32::MIN,
                operation: native.operation,
                ingress: IngressDialect::OpenAi,
                deployment,
                model,
                credential: Some(&secret),
                body: native.body,
                media: native.media,
                batch: native.batch,
                stream: false,
                capture: false,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(10),
                cancel: &cancel,
            }))
        });
        assert!(
            matches!(result, AttemptResult::Completed { .. }),
            "{result:?}"
        );
        bytes
    }

    /// Request validation accepts and refuses exactly what each operation's
    /// owned contract decodes, naming the same missing member or `body`, and
    /// returns the body unchanged; checking a body copies no message or
    /// extension text, so validation allocation does not grow with either at
    /// a fixed structure.
    ///
    /// # Panics
    ///
    /// Native encoding, measured separately, writes media JSON and multipart
    /// text, batch creation JSON, and batch upload lines straight into their
    /// final buffers: dispatch allocation grows only by those required bytes
    /// (a batch upload's file content, then the form carrying it), never by a
    /// copied body, line, or field.
    ///
    /// # Panics
    ///
    /// Panics when a projection differs from the pinned or owned outcome, the
    /// body changes, validation allocation grows with payload text, or native
    /// encoding allocates more than its required bytes.
    #[test]
    fn request_validation_preserves_contract_without_payload_copies() {
        let chat = |extra: Value| {
            let mut body =
                json!({"model": "acme/a", "messages": [{"role": "user", "content": "hi"}]});
            for (key, value) in extra.as_object().expect("members") {
                body[key] = value.clone();
            }
            body
        };
        let responses = |input: Value| json!({"model": "acme/a", "input": input});
        let cases: [(&str, Value, Result<(), &str>); 47] = [
            ("chat", chat(json!({})), Ok(())),
            ("chat", json!({"model": "acme/a"}), Err("messages")),
            ("chat", json!({"messages": []}), Err("model")),
            (
                "chat",
                chat(json!({"temperature": null, "top_p": 1})),
                Ok(()),
            ),
            (
                "chat",
                chat(json!({"max_tokens": 4_294_967_295_u64})),
                Ok(()),
            ),
            (
                "chat",
                chat(json!({"max_tokens": 4_294_967_296_u64})),
                Err("body"),
            ),
            ("chat", chat(json!({"max_tokens": -1})), Err("body")),
            ("chat", chat(json!({"max_tokens": 1.0})), Err("body")),
            (
                "chat",
                chat(json!({"x_extension": {"a": [1, null]}})),
                Ok(()),
            ),
            ("chat", chat(json!({"logit_bias": []})), Err("body")),
            (
                "chat",
                chat(json!({"logit_bias": null, "metadata": {"a": {}}})),
                Ok(()),
            ),
            (
                "chat",
                chat(json!({"audio": {"voice": "alloy", "format": "mp3"}})),
                Ok(()),
            ),
            (
                "chat",
                chat(json!({"audio": {"voice": {"id": "v"}, "format": "mp3", "x": 1}})),
                Err("body"),
            ),
            (
                "chat",
                chat(json!({"response_format": {"type": "json_object"}})),
                Ok(()),
            ),
            (
                "chat",
                chat(json!({"response_format": {"type": "xml"}})),
                Err("body"),
            ),
            (
                "chat",
                chat(json!({"stop": ["a", "b"], "tool_choice": "auto"})),
                Ok(()),
            ),
            ("chat", chat(json!({"stop": 5})), Err("body")),
            (
                "chat",
                chat(
                    json!({"messages": [{"role": "user", "content": [{"type": "text", "text": "x"}]}]}),
                ),
                Ok(()),
            ),
            (
                "chat",
                chat(json!({"messages": [{"content": "x"}]})),
                Err("role"),
            ),
            (
                "chat",
                chat(
                    json!({"tools": [{"type": "function", "function": {"name": "f", "parameters": {"type": "object"}}}]}),
                ),
                Ok(()),
            ),
            ("responses", responses(json!("hi")), Ok(())),
            ("responses", json!({"model": "acme/a"}), Err("input")),
            (
                "responses",
                responses(json!([{"role": "user", "content": "hi"}])),
                Ok(()),
            ),
            (
                "responses",
                responses(
                    json!([{"type": "message", "role": "assistant", "content": "x", "x": 1}]),
                ),
                Ok(()),
            ),
            (
                "responses",
                responses(json!([{"type": null, "role": "user", "content": "x"}])),
                Err("body"),
            ),
            (
                "responses",
                responses(json!([{"content": "x"}])),
                Err("body"),
            ),
            (
                "responses",
                responses(json!([{"role": "user"}])),
                Err("body"),
            ),
            (
                "responses",
                responses(
                    json!([{"type": "function_call", "call_id": "c", "name": "n", "arguments": "{}"}]),
                ),
                Ok(()),
            ),
            (
                "responses",
                responses(json!([{"type": "function_call", "call_id": "c", "name": "n"}])),
                Err("body"),
            ),
            (
                "responses",
                json!({"model": "acme/a", "input": "hi", "max_output_tokens": 1.5}),
                Err("body"),
            ),
            (
                "embeddings",
                json!({"model": "acme/a", "input": "a"}),
                Ok(()),
            ),
            (
                "embeddings",
                json!({"model": "acme/a", "input": [[1], [2]], "dimensions": null}),
                Ok(()),
            ),
            (
                "embeddings",
                json!({"model": "acme/a", "input": [1, "a"]}),
                Err("body"),
            ),
            (
                "embeddings",
                json!({"model": "acme/a", "input": "a", "x": 1}),
                Err("body"),
            ),
            ("embeddings", json!({"model": "acme/a"}), Err("input")),
            (
                "speech",
                json!({"model": "acme/a", "input": "a", "voice": "v", "speed": 1}),
                Ok(()),
            ),
            (
                "speech",
                json!({"model": "acme/a", "input": "a"}),
                Err("voice"),
            ),
            (
                "speech",
                json!({"model": "acme/a", "input": "a", "voice": "v", "speed": "x"}),
                Err("body"),
            ),
            (
                "speech",
                json!({"model": "acme/a", "input": "a", "voice": "v", "x": {}}),
                Ok(()),
            ),
            (
                "image",
                json!({"model": "acme/a", "prompt": "p", "n": 2}),
                Ok(()),
            ),
            (
                "image",
                json!({"model": "acme/a", "prompt": "p", "n": -1}),
                Err("body"),
            ),
            ("image", json!({"model": "acme/a"}), Err("prompt")),
            (
                "image",
                json!({"model": "acme/a", "prompt": "p", "size": null}),
                Ok(()),
            ),
            ("batch", sized("batch", "a", "b"), Ok(())),
            (
                "batch",
                json!({"input_file_id": "f", "endpoint": "e", "completion_window": "24h", "metadata": {"k": 1}}),
                Err("body"),
            ),
            (
                "batch",
                json!({"input_file_id": "f", "completion_window": "24h"}),
                Err("endpoint"),
            ),
            ("batch", json!([]), Err("body")),
        ];
        let decoders = decoders();
        for (operation, body, expected) in &cases {
            let (_, owned, validate) = decoders
                .iter()
                .find(|(name, ..)| name == operation)
                .expect("operation");
            let expected = expected.map_err(str::to_owned);
            assert_eq!(owned(body), expected, "{operation} owned {body}");
            assert_eq!(
                checked(*validate, body),
                expected,
                "{operation} checked {body}"
            );
        }

        let kib = |size: usize| "é".repeat(size << 9);
        for (operation, _, validate) in decoders {
            let allocated = |text: usize, extension: usize| {
                let body = sized(operation, &kib(text), &kib(extension));
                let input = Ok(JsonBody(arrived(&body)));
                let (checked, bytes) = CountingAllocator::measure(|| validate(input));
                assert_eq!(
                    checked.expect("valid").tree(),
                    &body,
                    "{operation} body unchanged"
                );
                bytes
            };
            let base = allocated(1024, 1024);
            for (label, grown) in [
                ("message", allocated(2048, 1024)),
                ("extension", allocated(1024, 2048)),
            ] {
                assert!(
                    grown.abs_diff(base) < 1024,
                    "{operation} validation copies {label} text: {base} then {grown} bytes"
                );
            }
        }

        let runtime = wyrd_runtime::runtime();
        let server = runtime.block_on(MockServer::start());
        runtime.block_on(
            Mock::given(wiremock::matchers::method("POST"))
                .respond_with(ResponseTemplate::new(200).set_body_raw("{}", "application/json"))
                .mount(&server),
        );
        let mut base = url::Url::parse(&server.uri()).expect("mock url");
        base.set_path("/v1");
        let dispatch = HttpProviderDispatch::new(
            skald_providers::EndpointPolicy::new(false),
            BuiltinEndpoints {
                openai: Some(base),
                ..BuiltinEndpoints::default()
            },
        )
        .expect("dispatch builds");
        let deployment: ProviderDeployment = serde_json::from_value(json!({
            "name": "dep",
            "models": ["gpt-4o"],
            "adapter": "openai",
            "auth": {"bearer": {"credential": "cred"}},
        }))
        .expect("deployment");
        let model = "openai/gpt-4o";
        let audio = UploadFile {
            field: "file".to_owned(),
            filename: "a.wav".to_owned(),
            content_type: "audio/wav".to_owned(),
            content: UploadContent::Bytes(vec![1, 2, 3]),
        };
        let encoded = |label: &str, required: usize, native: &dyn Fn(&str) -> usize| {
            let (small, large) = (kib(1024), kib(2048));
            let grown = native(&large).abs_diff(native(&small));
            let text = large.len() - small.len();
            assert!(
                grown < required * text + text / 8,
                "native encoding: {label} copies its text: {grown} bytes for {text} more text"
            );
        };
        encoded("media JSON", 1, &|text| {
            let body = json!({"model": model, "prompt": text, "size": "1024x1024"});
            let media = MediaRequest {
                route: OpenAiMediaRoute::ImageGenerations,
                files: Vec::new(),
            };
            dispatched(
                &dispatch,
                &deployment,
                &Native {
                    operation: GatewayOperation::Images,
                    body: &arrived(&body),
                    media: Some(&media),
                    batch: None,
                },
            )
        });
        encoded("media multipart", 1, &|text| {
            let body = json!({"model": model, "prompt": text, "temperature": 0.5});
            let media = MediaRequest {
                route: OpenAiMediaRoute::AudioTranscriptions,
                files: vec![audio.clone()],
            };
            dispatched(
                &dispatch,
                &deployment,
                &Native {
                    operation: GatewayOperation::Audio,
                    body: &RequestJson::from(body),
                    media: Some(&media),
                    batch: None,
                },
            )
        });
        encoded("batch create", 1, &|text| {
            let body = json!({
                "input_file_id": "file-1",
                "endpoint": "/v1/chat/completions",
                "completion_window": "24h",
                "metadata": {"note": text},
            });
            dispatched(
                &dispatch,
                &deployment,
                &Native {
                    operation: GatewayOperation::Batches,
                    body: &RequestJson::from(body),
                    media: None,
                    batch: Some(&BatchAction::CreateBatch),
                },
            )
        });
        encoded("batch upload", 2, &|text| {
            let line = json!({
                "custom_id": "a",
                "method": "POST",
                "url": "/v1/chat/completions",
                "body": {"model": model, "messages": [{"role": "user", "content": text}]},
            });
            let action = BatchAction::UploadFile {
                filename: "input.jsonl".to_owned(),
                input: BatchInput::parse(format!("{line}\n").as_bytes()).expect("input parses"),
            };
            dispatched(
                &dispatch,
                &deployment,
                &Native {
                    operation: GatewayOperation::Batches,
                    body: &RequestJson::from(json!({"purpose": "batch"})),
                    media: None,
                    batch: Some(&action),
                },
            )
        });

        // Planning checks every translated candidate without building its
        // payload: allocation grows with neither the text nor the number of
        // candidates.
        let planned = |text: &str, candidates: usize| {
            let names: Vec<String> = (0..candidates)
                .map(|index| format!("translated-{index}"))
                .collect();
            let mut deployments: Vec<ProviderDeployment> = names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let adapter = if index % 2 == 0 {
                        "anthropic"
                    } else {
                        "gemini"
                    };
                    serde_json::from_value(
                        json!({"name": name, "adapter": adapter, "auth": "none"}),
                    )
                    .expect("translated deployment")
                })
                .collect();
            deployments.sort_by(|a, b| a.name.cmp(&b.name));
            let models: Vec<ModelRef> = names
                .iter()
                .map(|name| ModelRef::from_projection(&format!("{name}/m")).expect("model"))
                .collect();
            let snapshot = GatewayTenantSnapshot {
                deployments,
                credentials: Vec::new(),
                fallback: serde_json::from_value(
                    json!({"rules": [{"scope": "global", "candidates": models}]}),
                )
                .expect("fallback"),
                governance: GatewayGovernancePolicy::default(),
                capture: serde_json::from_value(
                    json!({"mode": "disabled", "payload_fields": [], "version": 1}),
                )
                .expect("capture"),
            };
            let body = json!({
                "model": models[0].to_string(),
                "max_completion_tokens": 16,
                "messages": [
                    {"role": "system", "content": text},
                    {"role": "user", "content": [{"type": "text", "text": text}]},
                    {"role": "assistant", "content": text, "tool_calls": [{"id": "c", "type": "function", "function": {"name": "f", "arguments": format!("{{\"a\":{}}}", Value::String(text.to_owned()))}}]},
                    {"role": "tool", "tool_call_id": "c", "content": text},
                ],
            });
            // Translation reads the decoded body, which escaped arguments
            // need, so it is measured as planning receives it.
            let body = RequestJson::from(body);
            let mut plan = CallPlan::new(
                &snapshot,
                GatewayCallId::new_v7(),
                GatewayOperation::ChatCompletions,
                models[0].clone(),
                None,
            );
            let (planned, bytes) = CountingAllocator::measure(|| {
                plan.retain_representable(IngressDialect::OpenAi, false, &body, None)
            });
            planned.expect("every candidate represents the call");
            assert_eq!(plan.candidates.len(), candidates, "no candidate is dropped");
            bytes
        };
        let (small, large) = (kib(1024), kib(2048));
        let base = planned(&small, 2);
        for (label, grown) in [
            ("text", planned(&large, 2)),
            ("candidates", planned(&small, 4)),
        ] {
            assert!(
                grown.abs_diff(base) < small.len() / 8,
                "translated planning copies the request with more {label}: {base} then {grown} bytes"
            );
        }
    }

    /// One typed validation as an outcome: the checked body's decoded value,
    /// or the refused field.
    fn validated(validate: Validate, body: RequestJson) -> Result<Value, String> {
        match validate(Ok(JsonBody(body))) {
            Ok(checked) => Ok(checked.into_tree()),
            Err(WyrdError::GatewayInvalidRequest { details, .. }) => {
                Err(details["field"].as_str().unwrap_or("body").to_owned())
            }
            Err(error) => panic!("unexpected refusal {error}"),
        }
    }

    /// Native JSON ingress keeps a body's arrived text only when it is the
    /// compact serialization of its decoded value, and is then the decoded
    /// body for every reader: the same decode refusals, decoded value,
    /// members, typed validation of every native family, usage bounds of every
    /// bounded family, and provider bytes with or without a model override.
    /// Whitespace, escapes `serde_json` writes differently, duplicate names,
    /// renormalized numbers, deep nesting, and non-object bodies take the
    /// decoded path. Its JSON media types are exactly those [`Json`] accepts,
    /// and a large unescaped body is kept in the very buffer that arrived and
    /// is validated, bounded, and encoded without a decoded copy.
    ///
    /// # Panics
    ///
    /// Panics when any reader of a kept body differs from the decoded one,
    /// the kept decision differs from the expected one, a media type is judged
    /// unlike [`Json`] judges it, or the large body is copied or decoded.
    #[test]
    fn native_raw_ingress_matches_value_contract_and_bounds() {
        use axum::extract::FromRequest as _;
        use bytes::Bytes;

        let deep = |depth: usize| {
            format!(
                r#"{{"model":"acme/a","x":{}{}}}"#,
                "[".repeat(depth),
                "]".repeat(depth)
            )
        };
        let members = |count: usize| {
            let members: Vec<String> = (0..count)
                .map(|index| format!(r#""m{index}":{index}"#))
                .collect();
            format!("{{{}}}", members.join(","))
        };
        // `Some(kept)` pins the representation; `None` pins only agreement.
        let cases: Vec<(String, Option<bool>)> = [
            (r#"{"model":"acme/a","messages":[{"role":"user","content":"hi"}],"max_tokens":5,"n":2}"#, Some(true)),
            (r#"{"model":"acme/a","messages":[{"role":"user","content":"hi"}],"stream":true}"#, Some(true)),
            (r#"{ "model": "acme/a", "messages": [] }"#, Some(false)),
            ("{\"model\":\"acme/a\",\n\"input\":\"a\"}", Some(false)),
            (r#"{"model":"acme/a","input":"q\"s\\n\nt\tc\u0001"}"#, Some(true)),
            (r#"{"model":"acme/a","input":"\u00e9"}"#, Some(false)),
            (r#"{"model":"acme/a","input":"a\/b"}"#, Some(false)),
            (r#"{"model":"acme/a","input":"\u001F"}"#, Some(false)),
            (r#"{"model":"acme/a","input":"é ☃ 🦀 \u2028"}"#, Some(false)),
            ("{\"model\":\"acme/a\",\"input\":\"é ☃ 🦀 \u{2028}\"}", Some(true)),
            (r#"{"q\"name":1,"ключ":["ü"]}"#, Some(true)),
            (r#"{"model":"acme/a","model":"acme/b","input":"a"}"#, Some(false)),
            (r#"{"model":"acme/a","x":{"a":1,"a":2}}"#, Some(false)),
            (r#"{"model":"acme/a","x":[{"a":1},{"a":2}]}"#, Some(true)),
            (r#"{"model":"acme/a","max_tokens":1.0,"t":1.5,"p":0.1,"z":-0.0}"#, Some(true)),
            (r#"{"model":"acme/a","max_tokens":1e2}"#, Some(false)),
            (r#"{"model":"acme/a","max_tokens":10E0}"#, Some(false)),
            (r#"{"model":"acme/a","t":1.50}"#, Some(false)),
            (r#"{"model":"acme/a","max_tokens":-0}"#, None),
            (r#"{"model":"acme/a","max_tokens":18446744073709551615,"n":2}"#, Some(true)),
            (r#"{"model":"acme/a","max_tokens":18446744073709551616}"#, Some(false)),
            (r#"{"model":"acme/a","max_tokens":-9223372036854775808}"#, Some(true)),
            (r#"{"model":"acme/a","t":1e400}"#, None),
            (r#"{"model":"acme/a","t":1e-400}"#, None),
            (r#"{"model":"acme/a","input":"\ud800"}"#, None),
            (r#"{"model":"acme/a","input":"\ud83e\udd80"}"#, Some(false)),
            (r#"{"model":"acme/a","input":"a"} "#, Some(false)),
            (r#"{"model":"acme/a"}x"#, None),
            (r#"{"model":"acme/a"}{}"#, None),
            (r#"{"model":"acme/a""#, None),
            ("", None),
            (r#"{"model":"acme/a","input":null,"x":true,"y":false,"z":{},"w":[]}"#, Some(true)),
            ("{}", Some(true)),
            (r#"[{"model":"acme/a"}]"#, Some(false)),
            (r#""acme/a""#, Some(false)),
            ("null", Some(false)),
            (r#"{"contents":[{"parts":[{"text":"hi"}]}],"generationConfig":{"maxOutputTokens":8,"candidateCount":2}}"#, Some(true)),
            (r#"{"contents":[],"generation_config":{"max_output_tokens":8}}"#, Some(true)),
            (r#"{"model":"claude/m","max_tokens":4,"messages":[{"role":"user","content":"hi"}]}"#, Some(true)),
            (r#"{"model":"jev/m","input":{"text":"hi"}}"#, Some(true)),
        ]
        .into_iter()
        .map(|(text, kept)| (text.to_owned(), kept))
        .chain([
            (deep(120), Some(true)),
            (deep(126), None),
            (deep(127), None),
            (deep(400), None),
            (members(64), Some(true)),
            (members(65), Some(false)),
        ])
        .collect();

        let validators: Vec<(&str, Validate)> = decoders()
            .into_iter()
            .map(|(name, _, validate)| (name, validate))
            .chain([
                (
                    "anthropic",
                    typed_body::<GatewayAnthropicMessagesRequest> as Validate,
                ),
                ("gemini", typed_body::<GatewayGeminiGenerateContentRequest>),
                ("jev", typed_body::<GatewayJevRequest>),
            ])
            .collect();
        let bounds: [(&str, Bound); 5] = [
            ("chat", |body| {
                usage_bound(GatewayOperation::ChatCompletions, body)
            }),
            ("responses", |body| {
                usage_bound(GatewayOperation::Responses, body)
            }),
            ("embeddings", |body| {
                usage_bound(GatewayOperation::Embeddings, body)
            }),
            ("anthropic", anthropic_usage_bound),
            ("gemini", google_usage_bound),
        ];
        let mut tally = [0_usize; 3];
        for (text, kept) in &cases {
            let owned = serde_json::from_slice::<Value>(text.as_bytes());
            let parsed = RequestJson::parse(Bytes::copy_from_slice(text.as_bytes()));
            assert_eq!(parsed.is_ok(), owned.is_ok(), "decode agreement: {text}");
            let (Ok(parsed), Ok(owned)) = (parsed, owned) else {
                tally[2] += 1;
                continue;
            };
            let decoded = RequestJson::from(owned.clone());
            tally[usize::from(parsed.text().is_none())] += 1;
            if let Some(kept) = kept {
                assert_eq!(parsed.text().is_some(), *kept, "kept decision: {text}");
            }
            if let Some(kept) = parsed.text() {
                assert_eq!(kept, text.as_bytes(), "kept text is the arrived text");
                assert_eq!(
                    kept,
                    &serde_json::to_vec(&owned).expect("json")[..],
                    "kept text is the compact serialization: {text}"
                );
            }
            assert_eq!(parsed.tree(), &owned, "decoded value: {text}");
            assert_eq!(parsed.is_object(), owned.is_object(), "{text}");
            for name in ["model", "stream", "max_tokens", "input", "m63", "absent"] {
                assert_eq!(
                    parsed.member(name).as_deref(),
                    owned.get(name),
                    "member {name}: {text}"
                );
            }
            for (family, validate) in &validators {
                assert_eq!(
                    validated(*validate, parsed.clone()),
                    validated(*validate, decoded.clone()),
                    "{family} validation: {text}"
                );
            }
            for (family, bound) in &bounds {
                assert_eq!(
                    format!("{:?}", bound(&parsed)),
                    format!("{:?}", bound(&decoded)),
                    "{family} bound: {text}"
                );
            }
            if let Value::Object(members) = &owned {
                for model in [None, Some("native-m"), Some("q\"m")] {
                    assert_eq!(
                        parsed.to_json(model).expect("encodes"),
                        skald_providers::RequestBody::Object { members, model }
                            .to_json()
                            .expect("encodes"),
                        "provider bytes with model {model:?}: {text}"
                    );
                }
            }
        }
        assert_eq!(tally, [18, 20, 8], "kept, decoded, and refused bodies");

        // Media types and body refusals are exactly those of `Json`.
        let runtime = wyrd_runtime::runtime();
        let request = |content_type: Option<&[u8]>, body: &str| {
            let mut request = axum::extract::Request::new(Body::from(body.to_owned()));
            if let Some(content_type) = content_type {
                request.headers_mut().insert(
                    CONTENT_TYPE,
                    HeaderValue::from_bytes(content_type).expect("header value"),
                );
            }
            request
        };
        let judged = |content_type: Option<&[u8]>, body: &str| {
            let axum = runtime
                .block_on(Json::<Value>::from_request(
                    request(content_type, body),
                    &(),
                ))
                .is_ok();
            let kept = runtime
                .block_on(JsonBody::from_request(request(content_type, body), &()))
                .is_ok();
            (axum, kept)
        };
        let differing: Vec<_> = [
            Some(&b"application/json"[..]),
            Some(b"Application/JSON"),
            Some(b"application/json; charset=utf-8"),
            Some(b"application/json;charset=\"utf-8\""),
            Some(b"application/vnd.api+json"),
            Some(b"application/problem+JSON; v=1"),
            Some(b"application/+json"),
            Some(b"application/json+"),
            Some(b"application/jsonx"),
            Some(b"application/json "),
            Some(b" application/json"),
            Some(b"application/ json"),
            Some(b"application /json"),
            Some(b"application"),
            Some(b"application/"),
            Some(b"/json"),
            Some(b"text/json"),
            Some(b"text/plain"),
            Some(b"multipart/form-data; boundary=x"),
            Some(b"application/x-www-form-urlencoded"),
            Some(b"application/json, text/plain"),
            Some(b"application/ld+json;"),
            Some(b"application/a+b+json"),
            Some(b"application/a b+json"),
            Some(b"application/a/b+json"),
            Some(b"application/a@b+json"),
            Some(b"application/json ;charset=utf-8"),
            Some(b"application/json; charset"),
            Some(b"application/json;"),
            Some(b"application/json;;"),
            Some(b"application/json; charset=utf-8; q"),
            Some(b"application/json\t"),
            Some(b"application/json\xff"),
            Some(b""),
            None,
        ]
        .into_iter()
        .filter(|content_type| {
            let (axum, kept) = judged(*content_type, "{}");
            kept != axum
        })
        .map(|content_type| content_type.map(String::from_utf8_lossy))
        .collect();
        assert!(
            differing.is_empty(),
            "media types judged unlike Json: {differing:?}"
        );
        for (text, _) in &cases {
            let (axum, kept) = judged(Some(b"application/json"), text);
            assert_eq!(kept, axum, "body refusal: {text}");
        }

        // The 64 KiB unescaped body is kept in the buffer that arrived, and
        // validation, bounding, and encoding copy none of its text.
        let content = "x".repeat(64 << 10);
        let body = json!({
            "model": "acme/a",
            "messages": [{"role": "user", "content": content}],
            "max_tokens": 16,
        });
        let arrived_text = Bytes::from(serde_json::to_vec(&body).expect("json"));
        let (parsed, allocated) =
            CountingAllocator::measure(|| RequestJson::parse(arrived_text.clone()));
        let parsed = parsed.expect("decodes");
        assert!(allocated < 4096, "keeping copies text: {allocated} bytes");
        assert!(
            std::ptr::eq(parsed.text().expect("kept").as_ptr(), arrived_text.as_ptr()),
            "the kept text is the arrived buffer"
        );
        let (checked, allocated) = CountingAllocator::measure(|| {
            typed_body::<GatewayChatCompletionsRequest>(Ok(JsonBody(parsed)))
        });
        let checked = checked.expect("valid");
        assert!(
            allocated < 4096,
            "validation copies text: {allocated} bytes"
        );
        assert!(std::ptr::eq(
            checked.text().expect("still kept").as_ptr(),
            arrived_text.as_ptr()
        ));
        let (bound, allocated) = CountingAllocator::measure(|| {
            usage_bound(GatewayOperation::ChatCompletions, &checked).expect("bounded")
        });
        assert_eq!(bound[0].quantity.as_str(), arrived_text.len().to_string());
        assert!(allocated < 4096, "bounding copies text: {allocated} bytes");
        let (encoded, allocated) =
            CountingAllocator::measure(|| checked.to_json(Some("native-m")).expect("encodes"));
        assert!(
            allocated < encoded.len() + 1024,
            "encoding writes one buffer: {allocated} bytes for {}",
            encoded.len()
        );
        let mut expected = body;
        expected["model"] = json!("native-m");
        assert_eq!(encoded, serde_json::to_vec(&expected).expect("json"));
    }
}
