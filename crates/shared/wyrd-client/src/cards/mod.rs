//! Client-side Card registry operations.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

mod download;
mod download_progress;
mod engine;
mod error;
mod handle;
mod pull;
mod progress;
mod reads;
mod saga;

pub use engine::RegistryContext;
pub use handle::{CardSelector, Cards};
pub use pull::{
    CardGraphPuller, BundleArtifactManifest, BundleManifest, BundleCardManifest,
    BundleMode, PullSummary,
};
pub use progress::{DeclarationPhase, DeclarationProgressEvent, DeclarationProgressSink};
pub use wyrd_spec::envelope::CardKind;
pub use wyrd_spec::reference::CardRef;
pub use wyrd_spec::registry::{
    CardSummary, ListCardsRequest, ListCardsResponse, DeclarationReceipt,
};

/// Public surface shape of the `cards` module.
#[cfg(test)]
mod tests {
    use super::{CardSelector, Cards, ListCardsRequest};
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};

    /// The public handle, selector, and request types compose without staged-card or transport exports.
    #[test]
    fn public_surface_compiles_without_staged_card_or_transport_exports() {
        let _ = Cards::connect;
        let _ = Cards::connect_with;
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
