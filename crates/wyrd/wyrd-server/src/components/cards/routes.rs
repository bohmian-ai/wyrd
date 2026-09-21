//! Axum adapter for card registration.
//!
//! Authentication is enforced by the `/v1` middleware layer. The route keeps
//! the `card_write` permission check local because it is a capability decision
//! specific to this write operation, not an authentication decision shared by
//! every protected route.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use serde::Deserialize;
use wyrd_runtime::Permission;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::{CardName, CardUid, IdempotencyKey, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{
    CardRegistrationOutcome, CreateCardRequest, CreateCardResponse, DeleteCardResponse,
    GetCardResponse, ListCardsRequest, ListCardsResponse, ListVersionsResponse,
};
use wyrd_spec::storage::IDEMPOTENCY_KEY_HEADER;
use wyrd_spec::vala::api::AuditEvent;

use crate::audit;
use crate::components::auth::Caller;
use crate::components::cards::service;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build card routes for the `/v1` group.
pub fn cards_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_cards_http, register_card_http))
        .routes(routes!(get_card_http, delete_card_http))
        .routes(routes!(get_card_by_ref_http, delete_card_by_ref_http))
        .routes(routes!(get_latest_card_http))
        .routes(routes!(list_versions_http))
        .routes(routes!(list_artifacts_http))
        .routes(routes!(complete_card_http))
}

/// Fetch one Card by its exact kind-qualified UID.
#[utoipa::path(
    get,
    path = "/cards/by-uid/{kind}/{card_uid}",
    params(
        ("kind" = String, Path, description = "Card kind namespace"),
        ("card_uid" = String, Path, description = "Server-minted Card UID")
    ),
    responses(
        (status = 200, description = "Hydrated Card", body = GetCardResponse),
        (status = 400, description = "The Card kind, space, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.read.uid"))]
async fn get_card_http(
    State(state): State<AppState>,
    caller: Caller,
    Path((kind, card_uid)): Path<(String, String)>,
) -> Result<Json<GetCardResponse>, WyrdErrorResponse> {
    let kind = parse_card_kind(&kind)?;
    let card_uid = parse_card_uid(&card_uid)?;
    authorize_card_read(
        &state,
        &caller,
        "card.read.uid",
        &format!("card:{card_uid}"),
    )
    .await?;
    let response = service::get_card_by_uid(&state, &caller, &card_uid)
        .await
        .map_err(WyrdErrorResponse::from)?;
    if response.card.kind != kind {
        return Err(WyrdErrorResponse::from(WyrdError::registry_card_not_found(
            "card UID is not in the requested kind namespace",
        )));
    }
    Ok(Json(response))
}

/// Fetch one Card by its exact kind/space/name/version identity.
#[utoipa::path(
    get,
    path = "/cards/by-ref",
    params(
        ("kind" = String, Query, description = "Exact Card kind"),
        ("space" = String, Query, description = "Exact Card space"),
        ("name" = String, Query, description = "Exact Card name"),
        ("version" = String, Query, description = "Exact Card version")
    ),
    responses(
        (status = 200, description = "Hydrated Card", body = GetCardResponse),
        (status = 400, description = "The Card kind, space, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.read.ref"))]
async fn get_card_by_ref_http(
    State(state): State<AppState>,
    caller: Caller,
    Query(query): Query<CardRefQuery>,
) -> Result<Json<GetCardResponse>, WyrdErrorResponse> {
    let card_ref = query.into_card_ref()?;
    authorize_card_read(&state, &caller, "card.read.ref", &card_resource(&card_ref)).await?;
    service::get_card_by_ref(&state, &caller, &card_ref)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// Resolve the newest stable Active Card in an identity line.
#[utoipa::path(
    get,
    path = "/cards/{kind}/{space}/{name}/latest",
    params(
        ("kind" = String, Path, description = "Card kind"),
        ("space" = String, Path, description = "Card space"),
        ("name" = String, Path, description = "Card name")
    ),
    responses(
        (status = 200, description = "Latest Active Card", body = GetCardResponse),
        (status = 400, description = "The Card kind, space, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.read.latest"))]
async fn get_latest_card_http(
    State(state): State<AppState>,
    caller: Caller,
    Path((kind, space, name)): Path<(String, String, String)>,
) -> Result<Json<GetCardResponse>, WyrdErrorResponse> {
    let kind = parse_card_kind(&kind)?;
    let space = parse_space(&space)?;
    let name = parse_name(&name)?;
    authorize_card_read(
        &state,
        &caller,
        "card.read.latest",
        &format!("card:{}/{space}/{name}", kind.wire_name()),
    )
    .await?;
    service::get_latest_card(&state, &caller, kind, space, name)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// List versions in one exact Card identity line.
#[utoipa::path(
    get,
    path = "/cards/{kind}/{space}/{name}/versions",
    params(
        ("kind" = String, Path, description = "Card kind"),
        ("space" = String, Path, description = "Card space"),
        ("name" = String, Path, description = "Card name"),
        ("include_prerelease" = Option<bool>, Query, description = "Include prerelease versions")
    ),
    responses(
        (status = 200, description = "Card versions", body = ListVersionsResponse),
        (status = 400, description = "The Card kind, space, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
async fn list_versions_http(
    State(state): State<AppState>,
    caller: Caller,
    Path((kind, space, name)): Path<(String, String, String)>,
    Query(query): Query<VersionListQuery>,
) -> Result<Json<ListVersionsResponse>, WyrdErrorResponse> {
    let kind = parse_card_kind(&kind)?;
    let space = parse_space(&space)?;
    let name = parse_name(&name)?;
    authorize_card_read(
        &state,
        &caller,
        "card.read.versions",
        &format!("card:{}/{space}/{name}", kind.wire_name()),
    )
    .await?;
    service::list_card_versions(&state, &caller, kind, space, name, query.include_prerelease)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// List tenant-visible Card summaries with keyset pagination.
#[utoipa::path(
    get,
    path = "/cards",
    params(
        ("kind" = Option<String>, Query, description = "Filter by Card kind"),
        ("space" = Option<String>, Query, description = "Filter by Card space"),
        ("name" = Option<String>, Query, description = "Filter by Card name"),
        ("version_range" = Option<String>, Query, description = "Filter by semver range"),
        ("status" = Option<String>, Query, description = "Filter by lifecycle status"),
        ("filter" = Option<String>, Query, description = "Metadata query filter"),
        ("include_prerelease" = Option<bool>, Query, description = "Include prerelease versions"),
        ("limit" = Option<i32>, Query, description = "Maximum result count"),
        ("cursor" = Option<String>, Query, description = "Opaque continuation cursor")
    ),
    responses(
        (status = 200, description = "Card summaries", body = ListCardsResponse),
        (status = 400, description = "The list filter, limit, or cursor is not usable (WYRD_REGISTRY_400_LIST_LIMIT_OUT_OF_RANGE, WYRD_REGISTRY_400_CURSOR_MISMATCH, WYRD_QUERY_400_INVALID_FIELD, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
async fn list_cards_http(
    State(state): State<AppState>,
    caller: Caller,
    Query(query): Query<ListCardsRequest>,
) -> Result<Json<ListCardsResponse>, WyrdErrorResponse> {
    authorize_card_read(&state, &caller, "card.read.list", "cards").await?;
    service::list_cards(&state, &caller, query)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// List server-authoritative stored artifacts for one Card.
#[utoipa::path(
    get,
    path = "/cards/{card_uid}/artifacts",
    params(("card_uid" = String, Path, description = "Card UID")),
    responses(
        (status = 200, description = "Artifact inventory",
         body = wyrd_spec::registry::ArtifactInventoryResponse),
        (status = 400, description = "The Card kind, space, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
async fn list_artifacts_http(
    State(state): State<AppState>,
    caller: Caller,
    Path(card_uid): Path<String>,
) -> Result<Json<wyrd_spec::registry::ArtifactInventoryResponse>, WyrdErrorResponse> {
    let card_uid = parse_card_uid(&card_uid)?;
    authorize_card_read(
        &state,
        &caller,
        "card.read.artifacts",
        &format!("card:{card_uid}"),
    )
    .await?;
    service::list_card_artifacts(&state, &caller, &card_uid)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

#[utoipa::path(
    post,
    path = "/cards",
    request_body = CreateCardRequest,
    params(
        ("Idempotency-Key" = String, Header, description = "Stable key reused for retries", example = "card-register-001")
    ),
    responses(
        (status = 201, description = "Card registered",
         body = wyrd_spec::registry::CreateCardResponse),
        (status = 400, description = "The submission is not a valid Card, or its idempotency key is missing or malformed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_SPEC_TOO_LARGE, WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED, WYRD_REGISTRY_400_IDEMPOTENCY_KEY_INVALID, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not register Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 409, description = "The idempotency key was reused with a different \
          submission, or the version already exists (WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT, \
          WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 422, description = "A declared dependency does not resolve \
          (WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(
    skip(state, caller, headers, body),
    fields(operation = "card.registration.create")
)]
/// Accept a card registration request and return its durable registration result.
///
/// The required `Idempotency-Key` lets a caller safely retry after a lost
/// response. The service binds that key to the authenticated principal and
/// canonical request content, so an exact replay is safe while reuse for a
/// different request is rejected.
///
/// The `card:write` verdict is recorded exactly once: inside the service's
/// registration transaction when that transaction commits the Card, and
/// standalone otherwise — a missing or invalid `Idempotency-Key`, an exact
/// replay, a lost idempotency race, or any failure before commit.
///
/// # Errors
/// Returns `WYRD_PERMISSION_403_DENIED_RBAC` without `card:write`, the
/// audit-unavailable error when the verdict cannot be recorded, a validation
/// error for a missing or invalid `Idempotency-Key` or Card body, an
/// idempotency or version conflict, and registry or storage unavailability.
pub(crate) async fn register_card_http(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Json(body): Json<CreateCardRequest>,
) -> Result<(StatusCode, Json<wyrd_spec::registry::CreateCardResponse>), WyrdErrorResponse> {
    // `require_authenticated` on the `/v1` router has already verified the
    // access token and the `Caller` extractor has materialized its principal.
    // This check is intentionally route-local: authentication answers "who is
    // calling?" while this capability check answers "may they register cards?".
    let allowed = allow_card_write(&state, &caller, "card.registration.create", "cards").await?;
    let idempotency_key = match extract_required_idempotency_key(&headers) {
        Ok(key) => key,
        Err(error) => {
            audit::record_audit(state.postgres.vala_pool(), caller.data_tenant_id, &allowed)
                .await?;
            return Err(error);
        }
    };
    service::register_card(&state, &caller, idempotency_key.as_str(), body, &allowed)
        .await
        .map(|response| (StatusCode::CREATED, Json(response)))
        .map_err(WyrdErrorResponse::from)
}

/// Complete a Pending Card after the client-side storage transfer finishes.
///
/// The server rechecks every manifest and stores the immutable Card blob before
/// activating the Card. Repeating the request with the registration key is an
/// idempotent Active no-op; provider URLs and transfer details never cross
/// this lifecycle boundary.
#[utoipa::path(
    post,
    path = "/cards/{card_uid}/complete",
    params(
        ("card_uid" = String, Path, description = "Server-minted Card UID"),
        ("Idempotency-Key" = String, Header, description = "Registration key reused for retries", example = "card-register-001")
    ),
    responses(
        (status = 200, description = "Card completed", body = CreateCardResponse),
        (status = 400, description = "The Card UID or idempotency key is malformed, or an artifact never finished uploading (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_IDEMPOTENCY_KEY_INVALID, WYRD_REGISTRY_400_UPLOAD_INTERRUPTED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not write Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "The Card is no longer pending \
          (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 410, description = "The pending registration expired before completion \
          (WYRD_REGISTRY_410_OPERATION_EXPIRED)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem),
        (status = 507, description = "A stored artifact failed verification \
          (WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.registration.complete"))]
async fn complete_card_http(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Path(card_uid): Path<String>,
) -> Result<Json<CreateCardResponse>, WyrdErrorResponse> {
    let idempotency_key = extract_required_idempotency_key(&headers)?;
    let card_uid = parse_card_uid(&card_uid)?;
    authorize_card_write(
        &state,
        &caller,
        "card.registration.complete",
        &format!("card:{card_uid}"),
    )
    .await?;
    let outcome = service::complete_card(&state, &caller, &card_uid, idempotency_key.as_str())
        .await
        .map_err(WyrdErrorResponse::from)?;
    Ok(Json(single_card_response(outcome)))
}

/// Delete one Card by its exact UID.
///
/// The `card:write` verdict commits inside the soft-delete transaction; a delete
/// that commits nothing (not found, inbound references, or a failure) records it
/// standalone exactly once. Storage cleanup runs after that commit.
///
/// # Errors
/// Returns a validation error for an invalid kind or UID,
/// `WYRD_PERMISSION_403_DENIED_RBAC` without `card:write`, the audit-unavailable
/// error when the verdict cannot be recorded, not found, an inbound-reference
/// conflict, registry unavailability, and incomplete storage cleanup.
#[utoipa::path(
    delete,
    path = "/cards/by-uid/{kind}/{card_uid}",
    params(
        ("kind" = String, Path, description = "Card kind namespace"),
        ("card_uid" = String, Path, description = "Server-minted Card UID")
    ),
    responses(
        (status = 200, description = "Card deleted", body = DeleteCardResponse),
        (status = 400, description = "The Card kind, space, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not delete Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "Another Card still references this one \
          (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem),
        (status = 507, description = "Stored artifacts could not all be removed \
          (WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.registration.delete"))]
async fn delete_card_http(
    State(state): State<AppState>,
    caller: Caller,
    Path((kind, card_uid)): Path<(String, String)>,
) -> Result<Json<DeleteCardResponse>, WyrdErrorResponse> {
    let kind = CardKind::from_wire_name(&kind).ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(
            "kind is not a valid Card kind",
        ))
    })?;
    let card_uid = parse_card_uid(&card_uid)?;
    let allowed = allow_card_write(
        &state,
        &caller,
        "card.registration.delete",
        &format!("card:{card_uid}"),
    )
    .await?;
    service::delete_card_with_kind(&state, &caller, &card_uid, kind, &allowed)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// Delete one Card by its exact kind/space/name/version identity.
///
/// The `card:write` verdict commits inside the soft-delete transaction; a delete
/// that commits nothing (not found, inbound references, or a failure) records it
/// standalone exactly once. Storage cleanup runs after that commit.
///
/// # Errors
/// Returns a validation error for an invalid kind, space, name, or version,
/// `WYRD_PERMISSION_403_DENIED_RBAC` without `card:write`, the audit-unavailable
/// error when the verdict cannot be recorded, not found, an inbound-reference
/// conflict, registry unavailability, and incomplete storage cleanup.
#[utoipa::path(
    delete,
    path = "/cards/by-ref",
    params(
        ("kind" = String, Query, description = "Exact Card kind"),
        ("space" = String, Query, description = "Exact Card space"),
        ("name" = String, Query, description = "Exact Card name"),
        ("version" = String, Query, description = "Exact Card version")
    ),
    responses(
        (status = 200, description = "Card deleted", body = DeleteCardResponse),
        (status = 400, description = "The Card kind, space, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not delete Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "Another Card still references this one \
          (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem),
        (status = 507, description = "Stored artifacts could not all be removed \
          (WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.registration.delete"))]
async fn delete_card_by_ref_http(
    State(state): State<AppState>,
    caller: Caller,
    Query(query): Query<DeleteCardRefQuery>,
) -> Result<Json<DeleteCardResponse>, WyrdErrorResponse> {
    let card_ref = query.into_card_ref()?;
    let allowed = allow_card_write(
        &state,
        &caller,
        "card.registration.delete",
        &card_resource(&card_ref),
    )
    .await?;
    service::delete_card_by_ref(&state, &caller, &card_ref, &allowed)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

#[derive(Debug, Deserialize)]
struct DeleteCardRefQuery {
    kind: String,
    space: String,
    name: String,
    version: String,
}

#[derive(Debug, Deserialize)]
struct CardRefQuery {
    kind: String,
    space: String,
    name: String,
    version: String,
}

impl CardRefQuery {
    fn into_card_ref(self) -> Result<CardRef, WyrdErrorResponse> {
        Ok(CardRef {
            kind: parse_card_kind(&self.kind)?,
            space: Some(parse_space(&self.space)?),
            name: parse_name(&self.name)?,
            version: self.version.parse().map_err(|error| {
                WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(format!(
                    "version is invalid: {error}"
                )))
            })?,
            uid: None,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
struct VersionListQuery {
    #[serde(default)]
    include_prerelease: bool,
}

impl DeleteCardRefQuery {
    fn into_card_ref(self) -> Result<CardRef, WyrdErrorResponse> {
        let kind = CardKind::from_wire_name(&self.kind).ok_or_else(|| {
            WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(
                "kind is not a valid Card kind",
            ))
        })?;
        let space = SpaceName::new(self.space).map_err(|error| {
            WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(format!(
                "space is invalid: {error}"
            )))
        })?;
        let name = CardName::new(self.name).map_err(|error| {
            WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(format!(
                "name is invalid: {error}"
            )))
        })?;
        let version = self.version.parse().map_err(|error| {
            WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(format!(
                "version is invalid: {error}"
            )))
        })?;
        Ok(CardRef {
            kind,
            name,
            version,
            space: Some(space),
            uid: None,
        })
    }
}

fn single_card_response(outcome: CardRegistrationOutcome) -> CreateCardResponse {
    CreateCardResponse {
        root: outcome.card_ref.clone(),
        outcomes: vec![outcome],
        upload_plans: Vec::new(),
    }
}

/// Render one exact Card reference as a stable audit resource string.
///
/// Identity-line routes have no UID to name yet, so the decision row records the
/// kind/space/name/version the caller asked for. Keeping one renderer means every
/// such row is comparable across read and delete routes.
fn card_resource(card_ref: &CardRef) -> String {
    format!(
        "card:{}/{}/{}@{}",
        card_ref.kind.wire_name(),
        card_ref.space.as_ref().map_or("-", SpaceName::as_str),
        card_ref.name,
        card_ref.version,
    )
}

/// Evaluate and audit `card:write` for one receiving registry route.
///
/// The route-local check answers "may this principal change cards?" after
/// authentication answered "who is calling?". Delegating to [`audit::authorize`]
/// is what makes the verdict durable exactly once, before the route proceeds or
/// refuses, and fail-closed when the decision cannot be recorded.
async fn authorize_card_write(
    state: &AppState,
    caller: &Caller,
    operation: &str,
    resource: &str,
) -> Result<(), WyrdErrorResponse> {
    audit::authorize(
        state,
        caller,
        &Permission::card_write(),
        operation,
        resource,
    )
    .await
    .map_err(WyrdErrorResponse::from)
}

/// Evaluate `card:write`, audit a denial, and hand back the allowed verdict.
///
/// Registration and deletion own a SQL transaction the verdict must commit
/// with, so the allowed row is returned for the service to append there
/// rather than recorded here.
///
/// # Errors
/// Returns the public permission error for a denial, and
/// [`WyrdError::AuditUnavailable`] when the denial cannot be recorded.
async fn allow_card_write(
    state: &AppState,
    caller: &Caller,
    operation: &str,
    resource: &str,
) -> Result<AuditEvent, WyrdErrorResponse> {
    audit::authorize_recording_denial(
        state,
        caller,
        &Permission::card_write(),
        operation,
        resource,
    )
    .await
    .map_err(WyrdErrorResponse::from)
}

/// Evaluate and audit `card:read` for one receiving registry route.
///
/// Reads are audited on the same terms as writes: the decision row is the only
/// durable evidence that a principal was permitted to see a Card, so an allowed
/// read that cannot be recorded is refused rather than served.
async fn authorize_card_read(
    state: &AppState,
    caller: &Caller,
    operation: &str,
    resource: &str,
) -> Result<(), WyrdErrorResponse> {
    audit::authorize(state, caller, &Permission::card_read(), operation, resource)
        .await
        .map_err(WyrdErrorResponse::from)
}

fn parse_card_kind(value: &str) -> Result<CardKind, WyrdErrorResponse> {
    CardKind::from_wire_name(value).ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(
            "kind is not a valid Card kind",
        ))
    })
}

fn parse_space(value: &str) -> Result<SpaceName, WyrdErrorResponse> {
    SpaceName::new(value.to_owned()).map_err(|error| {
        WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(format!(
            "space is invalid: {error}"
        )))
    })
}

fn parse_name(value: &str) -> Result<CardName, WyrdErrorResponse> {
    CardName::new(value.to_owned()).map_err(|error| {
        WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(format!(
            "name is invalid: {error}"
        )))
    })
}

fn parse_card_uid(value: &str) -> Result<CardUid, WyrdErrorResponse> {
    CardUid::new(value).map_err(|error| {
        WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(format!(
            "card_uid is invalid: {error}"
        )))
    })
}

/// Extract and validate the required registration idempotency key.
///
/// Registration requires a caller-supplied key because the server cannot infer
/// whether two otherwise identical requests are a retry or an intentional new
/// operation. The service combines this key with the principal and canonical
/// request hash to provide exactly-once registration semantics.
fn extract_required_idempotency_key(
    headers: &HeaderMap,
) -> Result<IdempotencyKey, WyrdErrorResponse> {
    let value = headers
        .get(header::HeaderName::from_static(IDEMPOTENCY_KEY_HEADER))
        .ok_or_else(|| WyrdError::RegistryIdempotencyKeyRequired {
            message: "Idempotency-Key header is required for card lifecycle requests".to_owned(),
            details: serde_json::json!({ "header": IDEMPOTENCY_KEY_HEADER }),
        })?;
    let value = value
        .to_str()
        .map_err(|_| WyrdError::RegistryIdempotencyKeyInvalid {
            message: "idempotency key header is not valid UTF-8".to_owned(),
            details: serde_json::json!({ "header": IDEMPOTENCY_KEY_HEADER }),
        })?;
    IdempotencyKey::new(value).map_err(|error| WyrdError::RegistryIdempotencyKeyInvalid {
        message: "idempotency key is invalid".to_owned(),
        details: serde_json::json!({ "header": IDEMPOTENCY_KEY_HEADER, "reason": error.to_string() }),
    })
    .map_err(WyrdErrorResponse::from)
}

#[cfg(test)]
mod tests {
    use super::extract_required_idempotency_key;
    use axum::http::HeaderMap;

    /// Reject registration requests that omit the idempotency key header.
    #[test]
    fn registration_requires_idempotency_key() {
        let error = extract_required_idempotency_key(&HeaderMap::new())
            .expect_err("registration must require an idempotency key");

        assert_eq!(error.0.code(), "WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED");
        assert_eq!(error.0.status(), 400);
    }

    /// Reject malformed idempotency keys with a dedicated catalog code.
    #[test]
    fn registration_rejects_malformed_idempotency_key() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Idempotency-Key",
            "".parse().expect("test_setup: header parses"),
        );

        let error = extract_required_idempotency_key(&headers)
            .expect_err("test_setup: empty idempotency key must be rejected");

        assert_eq!(error.0.code(), "WYRD_REGISTRY_400_IDEMPOTENCY_KEY_INVALID");
        assert_eq!(error.0.status(), 400);
    }
}
