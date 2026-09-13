//! Keyset-paginated Card listing.

use crate::WyrdClient;
use wyrd_spec::registry::{ListCardsRequest, ListCardsResponse};

use crate::cards::error::RegistryEngineError;

/// List Cards with encoded query parameters and no GET request body.
pub(crate) async fn list(
    client: &WyrdClient,
    request: ListCardsRequest,
) -> Result<ListCardsResponse, RegistryEngineError> {
    let query = serde_urlencoded::to_string(&request)?;
    let path = if query.is_empty() {
        "/v1/cards".to_owned()
    } else {
        format!("/v1/cards?{query}")
    };
    client
        .request_json(reqwest::Method::GET, &path, None::<&()>)
        .await
        .map_err(Into::into)
}
