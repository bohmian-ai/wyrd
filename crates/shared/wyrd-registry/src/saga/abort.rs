//! Private CardUid compensation.

use wyrd_client::WyrdClient;
use wyrd_spec::registry::CreateCardResponse;

/// Best-effort compensation after a transfer or completion failure.
pub(crate) async fn compensate(
    client: &WyrdClient,
    response: &CreateCardResponse,
    _idempotency_key: &str,
) {
    for outcome in &response.outcomes {
        let Some(uid) = outcome.card_ref.uid.as_ref() else {
            continue;
        };
        let path = format!("/v1/cards/{uid}/abort");
        if let Err(error) = client
            .request_json::<(), CreateCardResponse>(reqwest::Method::POST, &path, None)
            .await
        {
            tracing::warn!(%error, %uid, "card registration compensation failed");
        }
    }
}
