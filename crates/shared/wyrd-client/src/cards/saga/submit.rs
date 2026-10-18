//! Typed declaration submission.

use crate::WyrdClient;
use wyrd_spec::registry::{DeclareCardRequest, DeclareCardResponse};

use crate::cards::error::RegistryEngineError;

/// Submit the flat card-declaration request with a stable idempotency key.
///
/// This helper lives in `submit` because it owns only the authenticated
/// `POST /v1/cards` transport call. The declaration saga calls it after local
/// preparation and before any artifact upload; `WyrdClient` owns transport and
/// error mapping while the server owns durable lifecycle state.
///
/// # Errors
///
/// Returns the transport error or structured server refusal for the
/// declaration request.
///
/// # Cancellation
///
/// Cancelling after the request is sent leaves declaration outcome unknown;
/// the request carries the saga's idempotency key for a keyed retry.
pub(crate) async fn submit_card_declaration(
    client: &WyrdClient,
    request: &DeclareCardRequest,
    idempotency_key: &str,
) -> Result<DeclareCardResponse, RegistryEngineError> {
    client
        .http
        .submit_with_idempotency_key(reqwest::Method::POST, "/v1/cards", request, idempotency_key)
        .await
        .map_err(Into::into)
}
