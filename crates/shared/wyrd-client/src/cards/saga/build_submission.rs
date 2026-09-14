//! Lower the loader's local-only registration input to the flat wire request.

use std::collections::BTreeMap;
use std::path::PathBuf;

use wyrd_loader::RegistrationInput;
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{CardSubmission, CreateCardRequest, RelativeArtifactPath};

use super::hash_artifacts;
use crate::cards::error::RegistryEngineError;

/// Prepared request plus the local artifact provenance retained for transfer.
pub(crate) struct PreparedRegistration {
    /// Wire-safe flat composite request.
    pub(crate) request: CreateCardRequest,
    /// Validated local sources keyed by manifest path.
    pub(crate) artifact_sources: BTreeMap<RelativeArtifactPath, PathBuf>,
}

/// Validate local sources, stamp client-owned manifest hashes, and project the
/// request without serializing any filesystem path.
///
/// Stamping works on a cloned submission set, so the caller's input is never
/// partially modified and no network call has been made when this returns.
///
/// # Errors
///
/// Returns the IO, size, or manifest-hash mismatch reported by
/// [`hash_artifacts::validate_and_stamp`], a serialization error when a
/// manifest cannot be canonicalized, and `RegistryUploadInterrupted` or
/// `RegistryInvalidArtifactPath` when declared artifacts and local sources do
/// not match one-to-one.
///
/// # Cancellation
///
/// Cancelling drops any open source file handle. Only local files have been
/// read, so there is no remote progress to reconcile.
pub(crate) async fn prepare(
    input: &RegistrationInput,
) -> Result<PreparedRegistration, RegistryEngineError> {
    let mut submissions = input.submissions.clone();
    for submission in &mut submissions {
        hash_artifacts::validate_and_stamp(submission, &input.artifact_sources).await?;
    }
    validate_source_set(&submissions, &input.artifact_sources)?;
    Ok(PreparedRegistration {
        request: CreateCardRequest { submissions },
        artifact_sources: input.artifact_sources.clone(),
    })
}

/// Require a one-to-one match between artifacts declared by the submissions
/// and the caller-supplied local sources.
///
/// Runs after every submission has been hashed and stamped, before any
/// registration network call, so a registration never starts with an
/// artifact that cannot be uploaded or a local file nobody declared.
///
/// # Errors
///
/// Returns `RegistryUploadInterrupted` when a declared artifact path has no
/// local source, and `RegistryInvalidArtifactPath` when a local source is not
/// declared by any submission.
fn validate_source_set(
    submissions: &[CardSubmission],
    sources: &BTreeMap<RelativeArtifactPath, PathBuf>,
) -> Result<(), RegistryEngineError> {
    let declared = submissions
        .iter()
        .flat_map(|submission| {
            submission
                .artifacts
                .iter()
                .map(|entry| &entry.relative_path)
        })
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(missing) = declared.iter().find(|path| !sources.contains_key(*path)) {
        return Err(WyrdError::RegistryUploadInterrupted {
            message: "a declared artifact has no local source".to_owned(),
            details: serde_json::json!({ "relative_path": missing.as_str() }),
        }
        .into());
    }
    if let Some(unexpected) = sources.keys().find(|path| !declared.contains(path)) {
        return Err(WyrdError::RegistryInvalidArtifactPath {
            message: "a local artifact source is not declared by any submission".to_owned(),
            details: serde_json::json!({ "relative_path": unexpected.as_str() }),
        }
        .into());
    }
    Ok(())
}
