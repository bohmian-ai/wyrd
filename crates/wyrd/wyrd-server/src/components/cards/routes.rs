//! Axum adapter for card registration.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::post;
use axum::{Json, Router};
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::registry::CreateCardRequest;
use wyrd_spec::storage::IDEMPOTENCY_KEY_HEADER;

use crate::components::auth::Caller;
use crate::components::cards::service;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Build card routes for the `/v1` group.
pub fn cards_router() -> Router<AppState> {
    Router::new().route("/cards", post(register_card_http))
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
pub(crate) async fn register_card_http(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Json(body): Json<CreateCardRequest>,
) -> Result<(StatusCode, Json<wyrd_spec::registry::CreateCardResponse>), WyrdErrorResponse> {
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

fn extract_required_idempotency_key(
    headers: &HeaderMap,
) -> Result<IdempotencyKey, WyrdErrorResponse> {
    let value = headers
        .get(header::HeaderName::from_static(IDEMPOTENCY_KEY_HEADER))
        .ok_or_else(|| WyrdError::RegistryIdempotencyKeyRequired {
            message: "Idempotency-Key header is required for card registration".to_owned(),
            details: serde_json::json!({ "header": IDEMPOTENCY_KEY_HEADER }),
        })?;
    let value = value.to_str().map_err(|_| WyrdError::Validation {
        message: "idempotency key header is not valid UTF-8".to_owned(),
        details: serde_json::json!({ "header": IDEMPOTENCY_KEY_HEADER }),
    })?;
    IdempotencyKey::new(value).map_err(|error| WyrdError::Validation {
        message: "idempotency key is invalid".to_owned(),
        details: serde_json::json!({ "header": IDEMPOTENCY_KEY_HEADER, "source": error.to_string() }),
    })
    .map_err(WyrdErrorResponse::from)
}

#[cfg(test)]
mod tests {
    use super::extract_required_idempotency_key;
    use axum::http::HeaderMap;

    #[test]
    fn registration_requires_idempotency_key() {
        let error = extract_required_idempotency_key(&HeaderMap::new())
            .expect_err("registration must require an idempotency key");

        assert_eq!(error.0.code(), "WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED");
        assert_eq!(error.0.status(), 400);
    }
}
