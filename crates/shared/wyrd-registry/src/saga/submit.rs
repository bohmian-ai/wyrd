//! Typed registration submission.

use wyrd_client::WyrdClient;
use wyrd_spec::registry::{CreateCardRequest, CreateCardResponse};

use crate::error::RegistryEngineError;

/// Submit the one flat composite request with a stable idempotency key.
pub(crate) async fn registration(
    client: &WyrdClient,
    request: &CreateCardRequest,
    idempotency_key: &str,
) -> Result<CreateCardResponse, RegistryEngineError> {
    client
        .submit_with_idempotency_key(reqwest::Method::POST, "/v1/cards", request, idempotency_key)
        .await
        .map_err(Into::into)
}
