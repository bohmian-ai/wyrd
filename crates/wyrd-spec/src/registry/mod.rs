//! Card registration wire contracts.

mod enums;
mod ids;
mod reads;
mod submission;

pub use enums::{CardLifecycleStatus, RegisterOutcome};
pub use ids::{PathValidationError, RegistrationOperationId, RelativeArtifactPath};
pub use reads::{
    ArtifactInventoryResponse, CardLocator, CardSummary, DeleteCardResponse, GetCardResponse,
    ListCardsRequest, ListCardsResponse, ListVersionsResponse, StoredArtifactEntry,
};
pub use submission::{
    ArtifactManifestEntry, CardSubmission, CreateCardRequest, CreateCardResponse,
    RegistrationReceipt, SubmissionMetadata,
};
