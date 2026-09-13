//! Latest Active Card resolution.

use crate::WyrdClient;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::GetCardResponse;

use super::card_ref_from_card;
use crate::cards::error::RegistryEngineError;

/// Resolve the latest Active version to an exact Card reference.
pub(crate) async fn resolve_latest(
    client: &WyrdClient,
    kind: CardKind,
    space: SpaceName,
    name: CardName,
) -> Result<CardRef, RegistryEngineError> {
    let path = format!(
        "/v1/cards/{}/{}/{}/latest",
        urlencoding::encode(kind.wire_name()),
        urlencoding::encode(space.as_str()),
        urlencoding::encode(name.as_str()),
    );
    let response: GetCardResponse = client
        .request_json(reqwest::Method::GET, &path, None::<&()>)
        .await?;
    card_ref_from_card(&response.card)
}
