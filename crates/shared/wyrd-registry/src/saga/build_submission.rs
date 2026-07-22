//! Lower the loader's local-only registration input to the flat wire request.

use std::collections::BTreeMap;
use std::path::PathBuf;

use wyrd_loader::RegistrationInput;
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{CardSubmission, CreateCardRequest, RelativeArtifactPath};

use super::hash_artifacts;
use crate::error::RegistryEngineError;

/// Prepared request plus the local artifact provenance retained for transfer.
pub(crate) struct PreparedRegistration {
    /// Wire-safe flat composite request.
    pub(crate) request: CreateCardRequest,
    /// Validated local sources keyed by manifest path.
    pub(crate) artifact_sources: BTreeMap<RelativeArtifactPath, PathBuf>,
}

/// Validate local sources, stamp client-owned manifest hashes, and project the
/// request without serializing any filesystem path.
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
