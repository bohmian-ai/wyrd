//! Axum adapter for card declaration.
//!
//! Authentication is enforced by the `/v1` middleware layer. The route keeps
//! the `card_write` permission check local because it is a capability decision
//! specific to this write operation, not an authentication decision shared by
//! every protected route.

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use wyrd_runtime::Permission;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::{CardName, CardUid, IdempotencyKey};
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{
    CardDeclarationOutcome, DeclareCardRequest, DeclareCardResponse, DeleteCardResponse,
    GetCardResponse, ListCardsRequest, ListCardsResponse, ListVersionsResponse,
};
use wyrd_spec::storage::IDEMPOTENCY_KEY_HEADER;

use crate::audit;
use crate::components::auth::Caller;
use crate::components::cards::service;
use crate::components::storage::inventory::CardObjects;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build card routes for the `/v1` group.
pub fn cards_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_cards_http, declare_card_http))
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
        (status = 400, description = "The Card kind, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
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
    Ok(Json(get_card_for(&state, &caller, &kind, &card_uid).await?))
}

/// Authorize, audit, and read one Card by its exact kind-qualified UID.
///
/// Shared by HTTP and the MCP `cards.get` tool so both audit `cards:read`
/// standalone and refuse a UID outside the requested kind identically.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] without `cards:read`,
/// [`WyrdError::RegistryCardNotFound`] when the UID is absent or of another
/// kind, and the registry errors of [`service::get_card_by_uid`].
pub(crate) async fn get_card_for(
    state: &AppState,
    caller: &Caller,
    kind: &CardKind,
    card_uid: &CardUid,
) -> Result<GetCardResponse, WyrdError> {
    audit::authorize(
        state,
        caller,
        &Permission::card_read(),
        "card.read.uid",
        &format!("card:{card_uid}"),
    )?;
    let response = service::get_card_by_uid(state, caller, card_uid).await?;
    if &response.card.kind != kind {
        return Err(WyrdError::registry_card_not_found(
            "card UID is not in the requested kind namespace",
        ));
    }
    Ok(response)
}

/// Fetch one Card by its exact kind/name/version identity.
///
/// The query names the flat identity alone; it is decoded strictly, so a
/// removed `space` selector is refused before authorization or any read.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an unknown, missing, or mistyped query
/// key, `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an ill-formed kind, name, or
/// version, and the permission and registry errors of
/// [`get_card_by_ref_for`].
#[utoipa::path(
    get,
    path = "/cards/by-ref",
    params(
        ("kind" = String, Query, description = "Exact Card kind"),
        ("name" = String, Query, description = "Exact Card name"),
        ("version" = String, Query, description = "Exact Card version")
    ),
    responses(
        (status = 200, description = "Hydrated Card", body = GetCardResponse),
        (status = 400, description = "The Card kind, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
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
    query: Result<Query<CardRefQuery>, QueryRejection>,
) -> Result<Json<GetCardResponse>, WyrdErrorResponse> {
    let card_ref = card_query(query)?.into_card_ref()?;
    Ok(Json(get_card_by_ref_for(&state, &caller, &card_ref).await?))
}

/// Authorize, audit, and read one Card by its exact identity.
///
/// Shared by HTTP and MCP so both audit `cards:read`
/// under the same exact-reference resource. A reference that also names a
/// UID must name the UID of the Card at that identity.
///
/// # Errors
/// Returns the errors of [`get_card_objects_by_ref_for`].
pub(crate) async fn get_card_by_ref_for(
    state: &AppState,
    caller: &Caller,
    card_ref: &CardRef,
) -> Result<GetCardResponse, WyrdError> {
    get_card_objects_by_ref_for(state, caller, card_ref)
        .await
        .map(|(response, _)| response)
}

/// [`get_card_by_ref_for`], also returning the Card's recorded objects its
/// hydration read, for a reader of the Card's files.
///
/// # Errors
/// Returns the errors of [`authorize_card_ref`],
/// [`WyrdError::RegistryCardNotFound`] when the named UID is another Card's,
/// and the errors of [`service::get_card_objects_by_ref`].
pub(crate) async fn get_card_objects_by_ref_for(
    state: &AppState,
    caller: &Caller,
    card_ref: &CardRef,
) -> Result<(GetCardResponse, CardObjects), WyrdError> {
    authorize_card_ref(state, caller, card_ref)?;
    let (response, objects) = service::get_card_objects_by_ref(state, caller, card_ref).await?;
    if let Some(uid) = &card_ref.uid
        && response.card.metadata.uid.as_ref() != Some(uid)
    {
        return Err(WyrdError::registry_card_not_found(
            "card UID does not match the referenced identity",
        ));
    }
    Ok((response, objects))
}

/// Authorize and audit the caller's `cards:read` of one exact Card
/// reference, the decision every read of that reference makes.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] without `cards:read`.
pub(crate) fn authorize_card_ref(
    state: &AppState,
    caller: &Caller,
    card_ref: &CardRef,
) -> Result<(), WyrdError> {
    audit::authorize(
        state,
        caller,
        &Permission::card_read(),
        "card.read.ref",
        &card_resource(card_ref),
    )?;
    Ok(())
}

/// Resolve the newest stable Active Card in an identity line.
///
/// The identity line is the tenant's flat `kind`/`name`; the audited resource
/// names that line because no version or UID is known yet.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an ill-formed kind or
/// name, `WYRD_PERMISSION_403_DENIED_RBAC` without `card:read`, and the
/// not-found and availability errors of [`service::get_latest_card`].
#[utoipa::path(
    get,
    path = "/cards/{kind}/{name}/latest",
    params(
        ("kind" = String, Path, description = "Card kind"),
        ("name" = String, Path, description = "Card name")
    ),
    responses(
        (status = 200, description = "Latest Active Card", body = GetCardResponse),
        (status = 400, description = "The Card kind, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
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
    Path((kind, name)): Path<(String, String)>,
) -> Result<Json<GetCardResponse>, WyrdErrorResponse> {
    let kind = parse_card_kind(&kind)?;
    let name = parse_name(&name)?;
    authorize_card_read(
        &state,
        &caller,
        "card.read.latest",
        &format!("card:{}/{name}", kind.wire_name()),
    )?;
    service::get_latest_card(&state, &caller, kind, name)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// List versions in one exact Card identity line.
///
/// The identity line is the tenant's flat `kind`/`name`.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an unknown or mistyped query key,
/// `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an ill-formed kind or
/// name, `WYRD_PERMISSION_403_DENIED_RBAC` without `card:read`, and the errors
/// of [`service::list_card_versions`].
#[utoipa::path(
    get,
    path = "/cards/{kind}/{name}/versions",
    params(
        ("kind" = String, Path, description = "Card kind"),
        ("name" = String, Path, description = "Card name"),
        ("include_prerelease" = Option<bool>, Query, description = "Include prerelease versions")
    ),
    responses(
        (status = 200, description = "Card versions", body = ListVersionsResponse),
        (status = 400, description = "The Card kind, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
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
    Path((kind, name)): Path<(String, String)>,
    query: Result<Query<VersionListQuery>, QueryRejection>,
) -> Result<Json<ListVersionsResponse>, WyrdErrorResponse> {
    let query = card_query(query)?;
    let kind = parse_card_kind(&kind)?;
    let name = parse_name(&name)?;
    authorize_card_read(
        &state,
        &caller,
        "card.read.versions",
        &format!("card:{}/{name}", kind.wire_name()),
    )?;
    service::list_card_versions(&state, &caller, kind, name, query.include_prerelease)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// List tenant-visible Card summaries with keyset pagination.
///
/// The strict [`ListCardsRequest`] query is decoded through [`card_query`], so
/// its decode refusals, including a removed `space` filter, answer as Wyrd
/// problems rather than framework text.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an unknown or mistyped query key and
/// the permission, filter, cursor, and registry errors of [`list_cards_for`].
#[utoipa::path(
    get,
    path = "/cards",
    params(
        ("kind" = Option<String>, Query, description = "Filter by Card kind"),
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
    query: Result<Query<ListCardsRequest>, QueryRejection>,
) -> Result<Json<ListCardsResponse>, WyrdErrorResponse> {
    Ok(Json(
        list_cards_for(&state, &caller, card_query(query)?).await?,
    ))
}

/// Authorize, audit, and list Cards for HTTP and MCP through the same operation.
///
/// # Errors
/// Returns a permission denial or the registry's filter, cursor, and storage errors.
pub(crate) async fn list_cards_for(
    state: &AppState,
    caller: &Caller,
    request: ListCardsRequest,
) -> Result<ListCardsResponse, WyrdError> {
    authorize_card_read(state, caller, "card.read.list", "cards")?;
    service::list_cards(state, caller, request).await
}

/// List server-authoritative stored artifacts for one Card.
#[utoipa::path(
    get,
    path = "/cards/{card_uid}/artifacts",
    params(("card_uid" = String, Path, description = "Card UID")),
    responses(
        (status = 200, description = "Artifact inventory",
         body = wyrd_spec::registry::ArtifactInventoryResponse),
        (status = 400, description = "The Card kind, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 404, description = "No such Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
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
    )?;
    service::list_card_artifacts(&state, &caller, &card_uid)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

#[utoipa::path(
    post,
    path = "/cards",
    request_body = DeclareCardRequest,
    params(
        ("Idempotency-Key" = String, Header, description = "Stable key reused for retries", example = "card-register-001")
    ),
    responses(
        (status = 201, description = "Card registered",
         body = wyrd_spec::registry::DeclareCardResponse),
        (status = 400, description = "The submission is not a valid Card, or its idempotency key is missing or malformed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_SPEC_TOO_LARGE, WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED, WYRD_REGISTRY_400_IDEMPOTENCY_KEY_INVALID, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not register Cards, or may not \
          invoke the Operators a binding's `on_failure` names \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)",
         body = WyrdProblem),
        (status = 409, description = "The idempotency key was reused with a different \
          submission, or the version already exists (WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT, \
          WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 422, description = "A declared dependency does not resolve \
          (WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(
    skip(state, caller, headers, body),
    fields(operation = "card.declaration.create")
)]
/// Accept a card declaration request and return its durable declaration result.
///
/// The required `Idempotency-Key` lets a caller safely retry after a lost
/// response. The service binds that key to the authenticated principal and
/// canonical request content, so an exact replay is safe while reuse for a
/// different request is rejected.
///
/// A request whose verification bindings name any `on_failure` Operator,
/// inline or referenced, also needs `operators:invoke`: the frozen Operator is
/// later dispatched by the system without another end-user decision, so the
/// declaring caller is the one authorized to create that outbound action.
/// Operator-free declaration evaluates `card:write` alone.
///
/// Every verdict is staged exactly once on the process audit outbox as soon as
/// it is decided, before the `Idempotency-Key` is read or the declaration
/// runs, so it is recorded whatever the declaration's outcome.
///
/// # Errors
/// Returns `WYRD_PERMISSION_403_DENIED_RBAC` without `card:write`, or without
/// `operators:invoke` for an Operator-bearing request, a validation
/// error for a missing or invalid `Idempotency-Key` or Card body, an
/// idempotency or version conflict, and registry or storage unavailability.
/// A body whose JSON does not match the flat envelope, such as one naming
/// `metadata.space`, is `WYRD_SPEC_400_VALIDATION` at location `body`;
/// malformed JSON, a missing JSON content type, and an oversized body keep
/// their native 400, 415, and typed 413 responses. A `space` inside a nested
/// reference in `spec` reaches the service decoder and is
/// `WYRD_REGISTRY_400_INVALID_CARD_SPEC`.
pub(crate) async fn declare_card_http(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    body: Result<Json<DeclareCardRequest>, JsonRejection>,
) -> Response {
    let body = match body {
        Ok(Json(body)) => body,
        Err(JsonRejection::JsonDataError(rejection)) => {
            return invalid_shape("body", rejection.body_text()).into_response();
        }
        Err(rejection) => return rejection.into_response(),
    };
    let declared: Result<_, WyrdErrorResponse> = async {
        // `require_authenticated` on the `/v1` router has already verified the
        // access token and the `Caller` extractor has materialized its principal.
        // This check is intentionally route-local: authentication answers "who is
        // calling?" while this capability check answers "may they declare cards?".
        authorize_card_write(&state, &caller, "card.declaration.create", "cards")?;
        if service::dispatches_operators(&body) {
            audit::authorize(
                &state,
                &caller,
                &Permission::operator_invoke(),
                "card.declaration.create",
                "operators",
            )?;
        }
        let idempotency_key = extract_required_idempotency_key(&headers)?;
        let response =
            service::declare_card(&state, &caller, idempotency_key.as_str(), body).await?;
        Ok((StatusCode::CREATED, Json(response)))
    }
    .await;
    declared.into_response()
}

/// Complete a Pending Card after the client-side storage transfer finishes.
///
/// The server rechecks every manifest and stores the immutable Card blob before
/// activating the Card. Repeating the request with the declaration key is an
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
        (status = 200, description = "Card completed", body = DeclareCardResponse),
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
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem),
        (status = 507, description = "A stored artifact failed verification \
          (WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(
    skip(state, caller, headers),
    fields(operation = "card.declaration.complete")
)]
async fn complete_card_http(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Path(card_uid): Path<String>,
) -> Result<Json<DeclareCardResponse>, WyrdErrorResponse> {
    let idempotency_key = extract_required_idempotency_key(&headers)?;
    let card_uid = parse_card_uid(&card_uid)?;
    authorize_card_write(
        &state,
        &caller,
        "card.declaration.complete",
        &format!("card:{card_uid}"),
    )?;
    let outcome = service::complete_card(&state, &caller, &card_uid, idempotency_key.as_str())
        .await
        .map_err(WyrdErrorResponse::from)?;
    Ok(Json(single_card_response(outcome)))
}

/// Delete one Card by its exact UID.
///
/// The `card:write` verdict is staged exactly once before the soft-delete
/// runs, whatever its outcome. Storage cleanup runs after the delete commits.
///
/// # Errors
/// Returns a validation error for an invalid kind or UID,
/// `WYRD_PERMISSION_403_DENIED_RBAC` without `card:write`, not found, an inbound-reference
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
        (status = 400, description = "The Card kind, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
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
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem),
        (status = 507, description = "Stored artifacts could not all be removed \
          (WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.declaration.delete"))]
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
    authorize_card_write(
        &state,
        &caller,
        "card.declaration.delete",
        &format!("card:{card_uid}"),
    )?;
    service::delete_card_with_kind(&state, &caller, &card_uid, kind)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// Delete one Card by its exact kind/name/version identity.
///
/// The `card:write` verdict is staged exactly once before the soft-delete
/// runs, whatever its outcome. Storage cleanup runs after the delete commits.
///
/// The query is decoded strictly: a removed `space` selector, or any other
/// unknown key, is refused before authorization, so it never deletes a Card.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an unknown, missing, or mistyped
/// query key, a validation error for an invalid kind, name, or version,
/// `WYRD_PERMISSION_403_DENIED_RBAC` without `card:write`, not found, an inbound-reference
/// conflict, registry unavailability, and incomplete storage cleanup.
#[utoipa::path(
    delete,
    path = "/cards/by-ref",
    params(
        ("kind" = String, Query, description = "Exact Card kind"),
        ("name" = String, Query, description = "Exact Card name"),
        ("version" = String, Query, description = "Exact Card version")
    ),
    responses(
        (status = 200, description = "Card deleted", body = DeleteCardResponse),
        (status = 400, description = "The Card kind, name, version, or UID is not well formed (WYRD_REGISTRY_400_INVALID_CARD_SPEC, WYRD_REGISTRY_400_VERSION_REQUIRED, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
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
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem),
        (status = 507, description = "Stored artifacts could not all be removed \
          (WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED)", body = WyrdProblem)
    ),
    tag = "Cards"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.declaration.delete"))]
async fn delete_card_by_ref_http(
    State(state): State<AppState>,
    caller: Caller,
    query: Result<Query<CardRefQuery>, QueryRejection>,
) -> Result<Json<DeleteCardResponse>, WyrdErrorResponse> {
    let card_ref = card_query(query)?.into_card_ref()?;
    authorize_card_write(
        &state,
        &caller,
        "card.declaration.delete",
        &card_resource(&card_ref),
    )?;
    service::delete_card_by_ref(&state, &caller, &card_ref)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// Exact `kind`/`name`/`version` selector of the `by-ref` read and delete routes.
///
/// It names the flat Card identity and nothing else: an unknown key, such as
/// the removed `space`, is refused before the selector reaches authorization,
/// the registry, or a delete, so an obsolete selector never acts on a Card.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CardRefQuery {
    /// Card kind wire name, such as `Prompt`.
    kind: String,
    /// Card name within the kind.
    name: String,
    /// One exact Card version.
    version: String,
}

impl CardRefQuery {
    /// Parse the selector into the UID-free [`CardRef`] the routes authorize.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when the kind, name, or
    /// version is not well formed.
    fn into_card_ref(self) -> Result<CardRef, WyrdErrorResponse> {
        Ok(CardRef {
            kind: parse_card_kind(&self.kind)?,
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

/// Query of the `versions` route; any other key, such as the removed `space`,
/// is refused.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionListQuery {
    /// Include prerelease versions in the listing.
    #[serde(default)]
    include_prerelease: bool,
}

/// Decode a Card route query, refusing a malformed one as a Wyrd problem.
///
/// A bare `Query` extractor answers with Axum's plain-text 400, which bypasses
/// the stable problem contract.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` at location `query` when a key is
/// unknown, such as the removed `space`, missing, or not the expected type.
fn card_query<T>(extracted: Result<Query<T>, QueryRejection>) -> Result<T, WyrdErrorResponse> {
    extracted
        .map(|Query(query)| query)
        .map_err(|rejection| invalid_shape("query", rejection.body_text()))
}

/// Refuse a Card request part the typed extractor could not decode.
///
/// The decoder's reason names the failing field path and the unknown or
/// missing key, so a caller sending the removed `space` learns which field to
/// drop. Card selectors and envelopes carry no secret material. Syntax,
/// content-type, and body-limit rejections never come here: they keep their
/// native 400, 415, and typed 413 responses.
fn invalid_shape(location: &str, reason: String) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Validation {
        message: format!(
            "card request {location} does not match the contract; Cards are named by kind, \
             name, and version"
        ),
        details: serde_json::json!({ "location": location, "reason": reason }),
    })
}

fn single_card_response(outcome: CardDeclarationOutcome) -> DeclareCardResponse {
    DeclareCardResponse {
        root: outcome.card_ref.clone(),
        outcomes: vec![outcome],
        upload_plans: Vec::new(),
    }
}

/// Render one exact Card reference as a stable audit resource string.
///
/// Identity-line routes have no UID to name yet, so the decision row records the
/// kind/name/version the caller asked for. Keeping one renderer means every
/// such row is comparable across read and delete routes.
fn card_resource(card_ref: &CardRef) -> String {
    format!(
        "card:{}/{}@{}",
        card_ref.kind.wire_name(),
        card_ref.name,
        card_ref.version,
    )
}

/// Evaluate and audit `card:write` for one receiving registry route.
///
/// The route-local check answers "may this principal change cards?" after
/// authentication answered "who is calling?". Delegating to [`audit::authorize`]
/// stages the verdict exactly once, before the route proceeds or refuses.
///
/// # Errors
/// Returns the public permission error when the caller lacks `card:write`.
fn authorize_card_write(
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
    .map_err(WyrdErrorResponse::from)
}

/// Evaluate and audit `card:read` for one receiving registry route.
///
/// Reads are audited on the same terms as writes: the decision row is the only
/// durable evidence that a principal was permitted to see a Card.
///
/// # Errors
/// Returns the public permission error when the caller lacks `card:read`.
fn authorize_card_read(
    state: &AppState,
    caller: &Caller,
    operation: &str,
    resource: &str,
) -> Result<(), WyrdErrorResponse> {
    audit::authorize(state, caller, &Permission::card_read(), operation, resource)
        .map_err(WyrdErrorResponse::from)
}

fn parse_card_kind(value: &str) -> Result<CardKind, WyrdErrorResponse> {
    CardKind::from_wire_name(value).ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(
            "kind is not a valid Card kind",
        ))
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

/// Extract and validate the required declaration idempotency key.
///
/// Declaration requires a caller-supplied key because the server cannot infer
/// whether two otherwise identical requests are a retry or an intentional new
/// operation. The service combines this key with the principal and canonical
/// request hash to provide exactly-once declaration semantics.
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

    /// Reject declaration requests that omit the idempotency key header.
    #[test]
    fn declaration_requires_idempotency_key() {
        let error = extract_required_idempotency_key(&HeaderMap::new())
            .expect_err("registration must require an idempotency key");

        assert_eq!(error.0.code(), "WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED");
        assert_eq!(error.0.status(), 400);
    }

    /// Reject malformed idempotency keys with a dedicated catalog code.
    #[test]
    fn declaration_rejects_malformed_idempotency_key() {
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
