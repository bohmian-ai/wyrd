use wyrd_registry::{CardSelector, Cards, ListCardsRequest};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};

#[test]
fn public_surface_compiles_without_staged_card_or_transport_exports() {
    let _ = Cards::new;
    let _ = Cards::with_client;
    let _ = CardSelector::named(
        CardKind::Prompt,
        SpaceName::new("prod").expect("test space is valid"),
        CardName::new("prompt").expect("test name is valid"),
    );
    let _ = ListCardsRequest {
        kind: None,
        space: None,
        name: None,
        version_range: None,
        status: None,
        filter: None,
        include_prerelease: false,
        limit: None,
        cursor: None,
    };
}
