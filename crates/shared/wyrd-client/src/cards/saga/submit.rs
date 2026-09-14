//! Typed registration submission.

use crate::WyrdClient;
use wyrd_spec::registry::{CreateCardRequest, CreateCardResponse};

use crate::cards::error::RegistryEngineError;

/// Submit the flat card-registration request with a stable idempotency key.
///
/// This helper lives in `submit` because it owns only the authenticated
/// `POST /v1/cards` transport call. The registration saga calls it after local
/// preparation and before any artifact upload; `WyrdClient` owns transport and
/// error mapping while the server owns durable lifecycle state.
///
/// # Errors
///
/// Returns the transport error or structured server refusal for the
/// registration request.
///
/// # Cancellation
///
/// Cancelling after the request is sent leaves registration outcome unknown;
/// the request carries the saga's idempotency key for a keyed retry.
pub(crate) async fn submit_card_registration(
    client: &WyrdClient,
    request: &CreateCardRequest,
    idempotency_key: &str,
) -> Result<CreateCardResponse, RegistryEngineError> {
    client
        .submit_with_idempotency_key(reqwest::Method::POST, "/v1/cards", request, idempotency_key)
        .await
        .map_err(Into::into)
}
