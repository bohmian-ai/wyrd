//! Private server-declared artifact inventory.

use crate::WyrdClient;
use wyrd_spec::ids::CardUid;
use wyrd_spec::registry::ArtifactInventoryResponse;

use crate::cards::error::RegistryEngineError;

/// Fetch the server-authoritative artifact inventory for one Card.
///
/// # Errors
///
/// Returns the transport error or structured server refusal for the lookup.
///
/// # Cancellation
///
/// The lookup is read-only; cancelling abandons the request.
pub(crate) async fn list_artifacts(
    client: &WyrdClient,
    card_uid: &CardUid,
) -> Result<ArtifactInventoryResponse, RegistryEngineError> {
    client
        .request_json(
            reqwest::Method::GET,
            &format!("/v1/cards/{card_uid}/artifacts"),
            None::<&()>,
        )
        .await
        .map_err(Into::into)
}
