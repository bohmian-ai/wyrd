//! Axum adapter for card registration.
//!
//! Authentication is enforced by the `/v1` middleware layer. The route keeps
//! the `card_write` permission check local because it is a capability decision
//! specific to this write operation, not an authentication decision shared by
//! every protected route.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
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

use crate::components::auth::Caller;
use crate::components::cards::service;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Build card routes for the `/v1` group.
pub fn cards_router() -> Router<AppState> {
    Router::new()
        .route("/cards", get(list_cards_http).post(register_card_http))
        .route(
            "/cards/by-uid/{kind}/{card_uid}",
            get(get_card_http).delete(delete_card_http),
        )
        .route(
            "/cards/by-ref",
            get(get_card_by_ref_http).delete(delete_card_by_ref_http),
        )
        .route(
            "/cards/{kind}/{space}/{name}/latest",
            get(get_latest_card_http),
        )
        .route(
            "/cards/{kind}/{space}/{name}/versions",
            get(list_versions_http),
        )
        .route("/cards/{card_uid}/artifacts", get(list_artifacts_http))
        .route("/cards/{card_uid}/complete", post(complete_card_http))
}

/// Fetch one Card by its exact kind-qualified UID.
#[utoipa::path(
    get,
    path = "/v1/cards/by-uid/{kind}/{card_uid}",
    params(
        ("kind" = String, Path, description = "Card kind namespace"),
        ("card_uid" = String, Path, description = "Server-minted Card UID")
    ),
    responses(
        (status = 200, description = "Hydrated Card", body = GetCardResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Card read permission required"),
        (status = 404, description = "Card not found"),
        (status = 503, description = "Registry unavailable")
    )
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.read.uid"))]
async fn get_card_http(
    State(state): State<AppState>,
    caller: Caller,
    Path((kind, card_uid)): Path<(String, String)>,
) -> Result<Json<GetCardResponse>, WyrdErrorResponse> {
    authorize_card_read(&state, &caller)?;
    let kind = parse_card_kind(&kind)?;
    let card_uid = parse_card_uid(&card_uid)?;
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
    path = "/v1/cards/by-ref",
    params(
        ("kind" = String, Query, description = "Exact Card kind"),
        ("space" = String, Query, description = "Exact Card space"),
        ("name" = String, Query, description = "Exact Card name"),
        ("version" = String, Query, description = "Exact Card version")
    ),
    responses(
        (status = 200, description = "Hydrated Card", body = GetCardResponse),
        (status = 404, description = "Card not found"),
        (status = 503, description = "Registry unavailable")
    )
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.read.ref"))]
async fn get_card_by_ref_http(
    State(state): State<AppState>,
    caller: Caller,
    Query(query): Query<CardRefQuery>,
) -> Result<Json<GetCardResponse>, WyrdErrorResponse> {
    authorize_card_read(&state, &caller)?;
    let card_ref = query.into_card_ref()?;
    service::get_card_by_ref(&state, &caller, &card_ref)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// Resolve the newest stable Active Card in an identity line.
#[utoipa::path(
    get,
    path = "/v1/cards/{kind}/{space}/{name}/latest",
    params(
        ("kind" = String, Path, description = "Card kind"),
        ("space" = String, Path, description = "Card space"),
        ("name" = String, Path, description = "Card name")
    ),
    responses(
        (status = 200, description = "Latest Active Card", body = GetCardResponse),
        (status = 404, description = "Card not found"),
        (status = 503, description = "Registry unavailable")
    )
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.read.latest"))]
async fn get_latest_card_http(
    State(state): State<AppState>,
    caller: Caller,
    Path((kind, space, name)): Path<(String, String, String)>,
) -> Result<Json<GetCardResponse>, WyrdErrorResponse> {
    authorize_card_read(&state, &caller)?;
    let kind = parse_card_kind(&kind)?;
    let space = parse_space(&space)?;
    let name = parse_name(&name)?;
    service::get_latest_card(&state, &caller, kind, space, name)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// List versions in one exact Card identity line.
#[utoipa::path(
    get,
    path = "/v1/cards/{kind}/{space}/{name}/versions",
    params(
        ("kind" = String, Path, description = "Card kind"),
        ("space" = String, Path, description = "Card space"),
        ("name" = String, Path, description = "Card name"),
        ("include_prerelease" = Option<bool>, Query, description = "Include prerelease versions")
    ),
    responses(
        (status = 200, description = "Card versions", body = ListVersionsResponse),
        (status = 503, description = "Registry unavailable")
    )
)]
async fn list_versions_http(
    State(state): State<AppState>,
    caller: Caller,
    Path((kind, space, name)): Path<(String, String, String)>,
    Query(query): Query<VersionListQuery>,
) -> Result<Json<ListVersionsResponse>, WyrdErrorResponse> {
    authorize_card_read(&state, &caller)?;
    service::list_card_versions(
        &state,
        &caller,
        parse_card_kind(&kind)?,
        parse_space(&space)?,
        parse_name(&name)?,
        query.include_prerelease,
    )
    .await
    .map(Json)
    .map_err(WyrdErrorResponse::from)
}

/// List tenant-visible Card summaries with keyset pagination.
#[utoipa::path(
    get,
    path = "/v1/cards",
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
        (status = 400, description = "Invalid list query", body = WyrdProblem),
        (status = 401, description = "Authentication required", body = WyrdProblem),
        (status = 403, description = "Card read permission required", body = WyrdProblem),
        (status = 503, description = "Registry unavailable", body = WyrdProblem)
    )
)]
async fn list_cards_http(
    State(state): State<AppState>,
    caller: Caller,
    Query(query): Query<ListCardsRequest>,
) -> Result<Json<ListCardsResponse>, WyrdErrorResponse> {
    authorize_card_read(&state, &caller)?;
    service::list_cards(&state, &caller, query)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// List server-authoritative stored artifacts for one Card.
#[utoipa::path(
    get,
    path = "/v1/cards/{card_uid}/artifacts",
    params(("card_uid" = String, Path, description = "Card UID")),
    responses(
        (status = 200, description = "Artifact inventory", body = wyrd_spec::registry::ArtifactInventoryResponse),
        (status = 404, description = "Card not found"),
        (status = 503, description = "Registry unavailable")
    )
)]
async fn list_artifacts_http(
    State(state): State<AppState>,
    caller: Caller,
    Path(card_uid): Path<String>,
) -> Result<Json<wyrd_spec::registry::ArtifactInventoryResponse>, WyrdErrorResponse> {
    authorize_card_read(&state, &caller)?;
    let card_uid = parse_card_uid(&card_uid)?;
    service::list_card_artifacts(&state, &caller, &card_uid)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

#[utoipa::path(
    post,
    path = "/v1/cards",
    request_body = CreateCardRequest,
    params(
        ("Idempotency-Key" = String, Header, description = "Stable key reused for retries", example = "card-register-001")
    ),
    responses(
        (status = 201, description = "Card registered", body = wyrd_spec::registry::CreateCardResponse),
        (status = 400, description = "Invalid request", body = WyrdProblem),
        (status = 401, description = "Authentication required", body = WyrdProblem),
        (status = 403, description = "Card write permission required", body = WyrdProblem),
        (status = 409, description = "Idempotency or version conflict", body = WyrdProblem),
        (status = 503, description = "Registry unavailable", body = WyrdProblem)
    )
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
    state
        .authz
        .permission_check
        .check(&caller.principal, &Permission::card_write())
        .into_result()
        .map_err(WyrdErrorResponse::from)?;
    let idempotency_key = extract_required_idempotency_key(&headers)?;
    service::register_card(&state, &caller, idempotency_key.as_str(), body)
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
    path = "/v1/cards/{card_uid}/complete",
    params(
        ("card_uid" = String, Path, description = "Server-minted Card UID"),
        ("Idempotency-Key" = String, Header, description = "Registration key reused for retries", example = "card-register-001")
    ),
    responses(
        (status = 200, description = "Card completed", body = CreateCardResponse),
        (status = 400, description = "Invalid Card UID or idempotency key", body = WyrdProblem),
        (status = 401, description = "Authentication required", body = WyrdProblem),
        (status = 403, description = "Card write permission required", body = WyrdProblem),
        (status = 404, description = "Card not found", body = WyrdProblem),
        (status = 409, description = "Card is not pending", body = WyrdProblem),
        (status = 503, description = "Registry unavailable", body = WyrdProblem),
        (status = 507, description = "Artifact verification failed", body = WyrdProblem)
    )
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.registration.complete"))]
async fn complete_card_http(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Path(card_uid): Path<String>,
) -> Result<Json<CreateCardResponse>, WyrdErrorResponse> {
    authorize_card_write(&state, &caller)?;
    let idempotency_key = extract_required_idempotency_key(&headers)?;
    let card_uid = parse_card_uid(&card_uid)?;
    let outcome = service::complete_card(&state, &caller, &card_uid, idempotency_key.as_str())
        .await
        .map_err(WyrdErrorResponse::from)?;
    Ok(Json(single_card_response(outcome)))
}

/// Delete one Card by its exact UID.
#[utoipa::path(
    delete,
    path = "/v1/cards/by-uid/{kind}/{card_uid}",
    params(
        ("kind" = String, Path, description = "Card kind namespace"),
        ("card_uid" = String, Path, description = "Server-minted Card UID")
    ),
    responses(
        (status = 200, description = "Card deleted", body = DeleteCardResponse),
        (status = 404, description = "Card not found"),
        (status = 409, description = "Card has inbound references"),
        (status = 503, description = "Registry unavailable"),
        (status = 507, description = "Storage cleanup incomplete")
    )
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.registration.delete"))]
async fn delete_card_http(
    State(state): State<AppState>,
    caller: Caller,
    Path((kind, card_uid)): Path<(String, String)>,
) -> Result<Json<DeleteCardResponse>, WyrdErrorResponse> {
    authorize_card_write(&state, &caller)?;
    let kind = CardKind::from_wire_name(&kind).ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::registry_invalid_card_spec(
            "kind is not a valid Card kind",
        ))
    })?;
    let card_uid = parse_card_uid(&card_uid)?;
    service::delete_card_with_kind(&state, &caller, &card_uid, kind)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// Delete one Card by its exact kind/space/name/version identity.
#[utoipa::path(
    delete,
    path = "/v1/cards/by-ref",
    params(
        ("kind" = String, Query, description = "Exact Card kind"),
        ("space" = String, Query, description = "Exact Card space"),
        ("name" = String, Query, description = "Exact Card name"),
        ("version" = String, Query, description = "Exact Card version")
    ),
    responses(
        (status = 200, description = "Card deleted", body = DeleteCardResponse),
        (status = 404, description = "Card not found"),
        (status = 409, description = "Card has inbound references"),
        (status = 503, description = "Registry unavailable"),
        (status = 507, description = "Storage cleanup incomplete")
    )
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.registration.delete"))]
async fn delete_card_by_ref_http(
    State(state): State<AppState>,
    caller: Caller,
    Query(query): Query<DeleteCardRefQuery>,
) -> Result<Json<DeleteCardResponse>, WyrdErrorResponse> {
    authorize_card_write(&state, &caller)?;
    let card_ref = query.into_card_ref()?;
    service::delete_card_by_ref(&state, &caller, &card_ref)
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

fn authorize_card_write(state: &AppState, caller: &Caller) -> Result<(), WyrdErrorResponse> {
    state
        .authz
        .permission_check
        .check(&caller.principal, &Permission::card_write())
        .into_result()
        .map_err(WyrdErrorResponse::from)
}

fn authorize_card_read(state: &AppState, caller: &Caller) -> Result<(), WyrdErrorResponse> {
    state
        .authz
        .permission_check
        .check(&caller.principal, &Permission::card_read())
        .into_result()
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
