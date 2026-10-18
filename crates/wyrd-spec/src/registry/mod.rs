//! Card declaration wire contracts.

mod enums;
/// Offline Card bundle wire projections.
mod bundle;
mod ids;
mod reads;
mod submission;
mod upload;

pub use crate::reference::{InlineableRef, Ref};
pub use enums::{CardLifecycleStatus, DeclarationOutcomeKind};
pub use bundle::{
    BundleArtifactManifest, BundleManifest, BundleCardManifest, BundleMode,
};
pub use ids::{PathValidationError, DeclarationOperationId, RelativeArtifactPath};
pub use reads::{
    ArtifactInventoryResponse, CardLocator, CardSummary, DeleteCardResponse, GetCardResponse,
    ListCardsRequest, ListCardsResponse, ListVersionsResponse, StoredArtifactEntry,
};
pub use submission::{
    ArtifactManifestEntry, CardDeclarationOutcome, CardSubmission, CardUploadPlan,
    DeclareCardRequest, DeclareCardResponse, DeclarationReceipt, DeclarationReplaySeed,
    canonical_artifact_manifest_hash,
};
pub use upload::CardUploadEntry;
