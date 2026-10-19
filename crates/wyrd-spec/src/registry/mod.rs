//! Card declaration wire contracts.

/// Offline Card bundle wire projections.
mod bundle;
mod enums;
mod ids;
mod reads;
mod submission;
mod upload;

pub use crate::reference::{InlineableRef, Ref};
pub use bundle::{BundleArtifactManifest, BundleCardManifest, BundleManifest, BundleMode};
pub use enums::{CardLifecycleStatus, DeclarationOutcomeKind};
pub use ids::{DeclarationOperationId, PathValidationError, RelativeArtifactPath};
pub use reads::{
    ArtifactInventoryResponse, CardLocator, CardSummary, DeleteCardResponse, GetCardResponse,
    ListCardsRequest, ListCardsResponse, ListVersionsResponse, StoredArtifactEntry,
};
pub use submission::{
    ArtifactManifestEntry, CardDeclarationOutcome, CardSubmission, CardUploadPlan,
    DeclarationReceipt, DeclarationReplaySeed, DeclareCardRequest, DeclareCardResponse,
    canonical_artifact_manifest_hash, validate_artifact_manifest,
};
pub use upload::CardUploadEntry;
