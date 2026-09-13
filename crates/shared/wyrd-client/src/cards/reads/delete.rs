//! Idempotent Card deletion.

use crate::WyrdClient;
use wyrd_spec::registry::DeleteCardResponse;

use crate::cards::error::RegistryEngineError;
use crate::cards::handle::CardSelector;
use crate::cards::reads::{get, selector_ref};

/// Resolve a selector when necessary and issue the idempotent delete request.
pub(crate) async fn delete(
    client: &WyrdClient,
    selector: CardSelector,
) -> Result<(), RegistryEngineError> {
    let path = match &selector {
        CardSelector::Uid {
            kind,
            space,
            name,
            version,
            uid,
        } => {
            if space.is_some() || name.is_some() || version.is_some() {
                get(client, &selector).await?;
            }
            format!(
                "/v1/cards/by-uid/{}/{}",
                urlencoding::encode(kind.wire_name()),
                urlencoding::encode(uid.as_str()),
            )
        }
        CardSelector::Named { .. } | CardSelector::Exact(_) => {
            let card_ref = selector_ref(&selector)?;
            format!(
                "/v1/cards/by-ref?kind={}&space={}&name={}&version={}",
                urlencoding::encode(card_ref.kind.wire_name()),
                urlencoding::encode(card_ref.space.as_ref().map_or("", |space| space.as_str()),),
                urlencoding::encode(card_ref.name.as_str()),
                urlencoding::encode(card_ref.version.as_str()),
            )
        }
    };
    let _response: DeleteCardResponse = client
        .request_json(reqwest::Method::DELETE, &path, None::<&()>)
        .await?;
    Ok(())
}
