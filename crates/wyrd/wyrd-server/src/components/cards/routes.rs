//! Axum adapter for card registration.
//!
//! Authentication is enforced by the `/v1` middleware layer. The route keeps
//! the `card_write` permission check local because it is a capability decision
//! specific to this write operation, not an authentication decision shared by
//! every protected route.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::post;
use axum::{Json, Router};
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, IdempotencyKey};
use wyrd_spec::registry::{CardRegistrationOutcome, CreateCardRequest, CreateCardResponse};
use wyrd_spec::storage::IDEMPOTENCY_KEY_HEADER;

use crate::components::auth::Caller;
use crate::components::cards::service;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Build card routes for the `/v1` group.
pub fn cards_router() -> Router<AppState> {
    Router::new()
        .route("/cards", post(register_card_http))
        .route("/cards/{card_uid}/complete", post(complete_card_http))
        .route("/cards/{card_uid}/abort", post(abort_card_http))
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
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Card write permission required"),
        (status = 409, description = "Idempotency or version conflict"),
        (status = 503, description = "Registry unavailable")
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
        (status = 404, description = "Card not found"),
        (status = 507, description = "Artifact verification failed")
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

/// Clean up an incomplete Card registration by Card UID.
///
/// Pending and Failed Cards may be retried through this endpoint. The server
/// attempts every manifest cleanup, records the Failed transition and audit
/// event transactionally, and returns a stable failure count without exposing
/// provider credentials or URLs.
#[utoipa::path(
    post,
    path = "/v1/cards/{card_uid}/abort",
    params(
        ("card_uid" = String, Path, description = "Server-minted Card UID"),
        ("Idempotency-Key" = String, Header, description = "Registration key reused for retries", example = "card-register-001")
    ),
    responses(
        (status = 200, description = "Card cleanup completed", body = CreateCardResponse),
        (status = 404, description = "Card not found"),
        (status = 409, description = "Card is already active")
    )
)]
#[tracing::instrument(skip(state, caller), fields(operation = "card.registration.abort"))]
async fn abort_card_http(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Path(card_uid): Path<String>,
) -> Result<Json<CreateCardResponse>, WyrdErrorResponse> {
    authorize_card_write(&state, &caller)?;
    let idempotency_key = extract_required_idempotency_key(&headers)?;
    let card_uid = parse_card_uid(&card_uid)?;
    let outcome = service::abort_card(&state, &caller, &card_uid, idempotency_key.as_str())
        .await
        .map_err(WyrdErrorResponse::from)?;
    Ok(Json(single_card_response(outcome)))
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
