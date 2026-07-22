//! CardUid completion through the server lifecycle endpoint.

use wyrd_client::WyrdClient;
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::CreateCardResponse;

use crate::error::RegistryEngineError;

/// Complete each artifact-bearing Card and merge the server-owned final state.
pub(crate) async fn cards(
    client: &WyrdClient,
    response: &CreateCardResponse,
    _idempotency_key: &str,
) -> Result<CreateCardResponse, RegistryEngineError> {
    let mut finalized = response.clone();
    for plan in &response.upload_plans {
        let uid = plan
            .card_ref
            .uid
            .as_ref()
            .ok_or_else(|| WyrdError::RegistryInvalidCardSpec {
                message: "server upload plan card reference has no Card UID".to_owned(),
                details: serde_json::json!({}),
            })?;
        let path = format!("/v1/cards/{uid}/complete");
        let completed: CreateCardResponse = client
            .request_json(reqwest::Method::POST, &path, None::<&()>)
            .await
            .map_err(RegistryEngineError::from)?;
        for completed_outcome in completed.outcomes {
            if let Some(existing) = finalized
                .outcomes
                .iter_mut()
                .find(|outcome| outcome.card_ref.same_identity(&completed_outcome.card_ref))
            {
                *existing = completed_outcome;
            }
        }
        finalized.root = completed.root;
    }
    Ok(finalized)
}
