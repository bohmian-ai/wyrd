//! Exact and latest Card lookup.

use wyrd_client::WyrdClient;
use wyrd_spec::envelope::Card;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::GetCardResponse;

use crate::error::RegistryEngineError;
use crate::handle::CardSelector;

/// Fetch a Card through the selector's exact or latest route.
pub(crate) async fn get(
    client: &WyrdClient,
    selector: &CardSelector,
) -> Result<Card, RegistryEngineError> {
    Ok(get_response(client, selector).await?.card)
}

/// Fetch a complete Card response while retaining server timestamps.
pub(crate) async fn get_response(
    client: &WyrdClient,
    selector: &CardSelector,
) -> Result<GetCardResponse, RegistryEngineError> {
    let response: GetCardResponse = match selector {
        CardSelector::Uid { kind, uid, .. } => {
            let path = format!("/v1/cards/by-uid/{}/{}", kind.wire_name(), uid);
            client
                .request_json(reqwest::Method::GET, &path, None::<&()>)
                .await?
        }
        CardSelector::Named {
            kind,
            space,
            name,
            version: Some(version),
        } => {
            let path = format!(
                "/v1/cards/by-ref?kind={}&space={}&name={}&version={}",
                urlencoding::encode(kind.wire_name()),
                urlencoding::encode(space.as_str()),
                urlencoding::encode(name.as_str()),
                urlencoding::encode(version.as_str()),
            );
            client
                .request_json(reqwest::Method::GET, &path, None::<&()>)
                .await?
        }
        CardSelector::Exact(card_ref) => {
            let space =
                card_ref
                    .space
                    .as_ref()
                    .ok_or_else(|| WyrdError::RegistryInvalidCardSpec {
                        message: "exact CardRef is missing its resolved space".to_owned(),
                        details: serde_json::json!({}),
                    })?;
            let path = format!(
                "/v1/cards/by-ref?kind={}&space={}&name={}&version={}",
                urlencoding::encode(card_ref.kind.wire_name()),
                urlencoding::encode(space.as_str()),
                urlencoding::encode(card_ref.name.as_str()),
                urlencoding::encode(card_ref.version.as_str()),
            );
            client
                .request_json(reqwest::Method::GET, &path, None::<&()>)
                .await?
        }
        CardSelector::Named {
            kind,
            space,
            name,
            version: None,
        } => {
            let path = format!(
                "/v1/cards/{}/{}/{}/latest",
                urlencoding::encode(kind.wire_name()),
                urlencoding::encode(space.as_str()),
                urlencoding::encode(name.as_str()),
            );
            client
                .request_json(reqwest::Method::GET, &path, None::<&()>)
                .await?
        }
    };
    assert_selector_identity(selector, &response.card)?;
    Ok(response)
}

/// Convert an exact selector into the wire reference shape.
pub(crate) fn selector_ref(selector: &CardSelector) -> Result<CardRef, RegistryEngineError> {
    match selector {
        CardSelector::Named {
            kind,
            space,
            name,
            version: Some(version),
        } => Ok(CardRef {
            kind: kind.clone(),
            name: name.clone(),
            version: version.clone(),
            space: Some(space.clone()),
            uid: None,
        }),
        CardSelector::Exact(card_ref) => {
            if card_ref.space.is_none() {
                return Err(WyrdError::RegistryInvalidCardSpec {
                    message: "exact CardRef is missing its resolved space".to_owned(),
                    details: serde_json::json!({}),
                }
                .into());
            }
            Ok(card_ref.clone())
        }
        CardSelector::Named { version: None, .. } => Err(WyrdError::RegistryVersionRequired {
            message: "an exact version is required for this operation".to_owned(),
            details: serde_json::json!({}),
        }
        .into()),
        CardSelector::Uid { .. } => Err(WyrdError::RegistryInvalidCardSpec {
            message: "a UID selector is not an exact named reference".to_owned(),
            details: serde_json::json!({}),
        }
        .into()),
    }
}

/// Assert optional UID-selector identity fields against the server response.
pub(crate) fn assert_selector_identity(
    selector: &CardSelector,
    card: &Card,
) -> Result<(), RegistryEngineError> {
    let matches = match selector {
        CardSelector::Uid {
            kind,
            uid,
            space,
            name,
            version,
        } => {
            card.kind == *kind
                && card.metadata.uid.as_ref() == Some(uid)
                && space
                    .as_ref()
                    .is_none_or(|expected| card.metadata.space.as_ref() == Some(expected))
                && name
                    .as_ref()
                    .is_none_or(|expected| card.metadata.name == *expected)
                && version
                    .as_ref()
                    .is_none_or(|expected| card.metadata.resolved_pin() == Some(expected))
        }
        CardSelector::Named {
            kind,
            space,
            name,
            version: Some(version),
        } => {
            card.kind == *kind
                && card.metadata.space.as_ref() == Some(space)
                && card.metadata.name == *name
                && card.metadata.resolved_pin() == Some(version)
        }
        CardSelector::Exact(expected) => {
            card.kind == expected.kind
                && card.metadata.space.as_ref() == expected.space.as_ref()
                && card.metadata.name == expected.name
                && card.metadata.resolved_pin() == Some(&expected.version)
                && expected
                    .uid
                    .as_ref()
                    .is_none_or(|uid| card.metadata.uid.as_ref() == Some(uid))
        }
        CardSelector::Named { version: None, .. } => true,
    };
    if matches {
        Ok(())
    } else {
        Err(WyrdError::RegistryCardRefUidNotResolvableHere {
            message: "the server response does not match the requested Card UID identity"
                .to_owned(),
            details: serde_json::json!({}),
        }
        .into())
    }
}

/// Convert a hydrated server Card into an exact resolved reference.
pub(crate) fn card_ref_from_card(card: &Card) -> Result<CardRef, RegistryEngineError> {
    let Some(version) = card.metadata.resolved_pin().cloned() else {
        return Err(WyrdError::RegistryInvalidVersionBlock {
            message: "server response did not contain a resolved version".to_owned(),
            details: serde_json::json!({}),
        }
        .into());
    };
    let Some(space) = card.metadata.space.clone() else {
        return Err(WyrdError::RegistryInvalidCardSpec {
            message: "server response did not contain a resolved space".to_owned(),
            details: serde_json::json!({}),
        }
        .into());
    };
    Ok(CardRef {
        kind: card.kind.clone(),
        name: card.metadata.name.clone(),
        version,
        space: Some(space),
        uid: card.metadata.uid.clone(),
    })
}
