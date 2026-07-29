//! Card registration wire contracts.

mod enums;
/// Offline hydrated-bundle wire projections.
mod hydrated;
mod ids;
mod reads;
mod submission;
mod upload;

pub use crate::reference::{InlineableRef, Ref};
pub use enums::{CardLifecycleStatus, RegistrationOutcomeKind};
pub use hydrated::{
    HydratedArtifactManifest, HydratedBundleManifest, HydratedCardManifest, HydrationMode,
};
pub use ids::{PathValidationError, RegistrationOperationId, RelativeArtifactPath};
pub use reads::{
    ArtifactInventoryResponse, CardLocator, CardSummary, DeleteCardResponse, GetCardResponse,
    ListCardsRequest, ListCardsResponse, ListVersionsResponse, StoredArtifactEntry,
};
pub use submission::{
    ArtifactManifestEntry, CardRegistrationOutcome, CardSubmission, CardUploadPlan,
    CreateCardRequest, CreateCardResponse, RegistrationReceipt, RegistrationReplaySeed,
    canonical_artifact_manifest_hash,
};
pub use upload::CardUploadEntry;
