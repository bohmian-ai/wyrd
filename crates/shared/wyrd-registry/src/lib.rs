//! Client-side Card registry operations.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

mod config;
mod download;
mod engine;
mod error;
mod handle;
mod hydrate;
mod progress;
mod reads;
mod saga;

pub use engine::RegistryContext;
pub use handle::{CardSelector, Cards, LoadedCard};
pub use hydrate::{
    CardGraphHydrator, HydratedArtifactManifest, HydratedBundleManifest, HydratedCardManifest,
    HydrationMode, HydrationSummary,
};
pub use progress::{RegistrationPhase, RegistrationProgressEvent, RegistrationProgressSink};
pub use wyrd_spec::registry::{
    CardSummary, ListCardsRequest, ListCardsResponse, RegistrationReceipt,
};

#[cfg(test)]
mod tests {
    use super::{CardSelector, Cards, ListCardsRequest};
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
}
