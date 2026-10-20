//! Optional declaration progress events for caller-owned presentation.

use std::sync::Arc;

use wyrd_spec::registry::RelativeArtifactPath;

/// A declaration lifecycle phase that does not report byte progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationPhase {
    /// Local input is being validated and lowered to the wire request.
    Preparing,
    /// The composite card request is being submitted to the server.
    Submitting,
    /// Artifact bytes are being transferred.
    Uploading,
    /// The server is verifying and completing uploaded artifacts.
    Completing,
    /// The saga is checking that every declared Card is Active.
    Verifying,
    /// Best-effort cleanup is running after a failed saga stage.
    CleaningUp,
}

/// Typed progress emitted by the declaration saga.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationProgressEvent {
    /// The saga entered a non-upload lifecycle phase.
    Phase(DeclarationPhase),
    /// One artifact became eligible for an active progress row.
    ArtifactStarted {
        /// Stable artifact identity within the registration request.
        artifact: RelativeArtifactPath,
        /// Expected byte count when the source exposes one.
        total_bytes: Option<u64>,
    },
    /// One artifact transferred more bytes.
    ArtifactProgress {
        /// Stable artifact identity within the registration request.
        artifact: RelativeArtifactPath,
        /// Bytes consumed by the provider transfer.
        uploaded_bytes: u64,
        /// Expected byte count, when known.
        total_bytes: Option<u64>,
    },
    /// One artifact transfer ended and its row can be removed.
    ArtifactFinished {
        /// Stable artifact identity within the registration request.
        artifact: RelativeArtifactPath,
        /// Whether the artifact transfer completed successfully.
        success: bool,
    },
}

/// Thread-safe callback owned by the declaration caller.
pub type DeclarationProgressSink = Arc<dyn Fn(DeclarationProgressEvent) + Send + Sync + 'static>;
