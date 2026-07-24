//! Bounded artifact upload orchestration.
//!
//! The registry owns only the relationship between local authored sources and
//! the server's upload entries. [`WyrdStorageClient`] owns all transfer
//! protocol details, including provider dispatch, part URL minting, backend
//! completion, and upload outcome handling.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::stream::{self, StreamExt};
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{
    CardLifecycleStatus, CardUploadEntry, CreateCardResponse, RelativeArtifactPath,
};
use wyrd_storage_client::{
    ArtifactSource, FileSource, UploadProgress, UploadProgressSink, WyrdStorageClient,
};

use crate::error::RegistryEngineError;
use crate::progress::{RegistrationProgressEvent, RegistrationProgressSink};

/// Transfer every server-minted artifact entry with bounded concurrency.
pub(crate) async fn upload_artifacts(
    storage: &WyrdStorageClient,
    response: &CreateCardResponse,
    sources: &BTreeMap<RelativeArtifactPath, PathBuf>,
    idempotency_key: &str,
    progress: Option<RegistrationProgressSink>,
) -> Result<(), RegistryEngineError> {
    let entries = collect_upload_entries(response);
    if entries.is_empty()
        && !response.outcomes.is_empty()
        && response
            .outcomes
            .iter()
            .all(|outcome| outcome.status == CardLifecycleStatus::Active)
    {
        return Ok(());
    }
    validate_artifact_sources(&entries, sources)?;

    let results = stream::iter(entries)
        .map(|entry| {
            let progress = progress.clone();
            async move {
                let source = sources
                    .get(&entry.relative_path)
                    .ok_or_else(|| WyrdError::internal("validated artifact source disappeared"))?;
                let result = upload_artifact_entry(
                    storage,
                    &entry,
                    source,
                    idempotency_key,
                    progress.clone(),
                )
                .await;
                if let Some(progress) = &progress {
                    progress(RegistrationProgressEvent::ArtifactFinished {
                        artifact: entry.relative_path.clone(),
                        success: result.is_ok(),
                    });
                }
                result
            }
        })
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;
    for result in results {
        result?;
    }
    Ok(())
}

/// Flatten server upload plans into their typed upload entries.
fn collect_upload_entries(response: &CreateCardResponse) -> Vec<CardUploadEntry> {
    response
        .upload_plans
        .iter()
        .flat_map(|plan| plan.entries.iter().cloned())
        .collect()
}

/// Require exact equality between planned relative paths and local sources.
fn validate_artifact_sources(
    entries: &[CardUploadEntry],
    sources: &BTreeMap<RelativeArtifactPath, PathBuf>,
) -> Result<(), RegistryEngineError> {
    let planned = entries
        .iter()
        .map(|entry| &entry.relative_path)
        .collect::<BTreeSet<_>>();
    if let Some(missing) = sources.keys().find(|path| !planned.contains(path)) {
        return Err(WyrdError::RegistryUploadInterrupted {
            message: "the server did not return an upload plan for a declared artifact".to_owned(),
            details: serde_json::json!({ "relative_path": missing.as_str() }),
        }
        .into());
    }
    if let Some(unexpected) = planned.iter().find(|path| !sources.contains_key(path)) {
        return Err(WyrdError::RegistryUploadInterrupted {
            message: "the server returned an upload plan for an unknown artifact".to_owned(),
            details: serde_json::json!({ "relative_path": unexpected.as_str() }),
        }
        .into());
    }
    Ok(())
}

/// Open one local source and hand it to the storage-client façade.
async fn upload_artifact_entry(
    storage: &WyrdStorageClient,
    entry: &CardUploadEntry,
    source: &Path,
    idempotency_key: &str,
    progress: Option<RegistrationProgressSink>,
) -> Result<(), RegistryEngineError> {
    let source = FileSource::open(source).await?;
    let total_bytes = source.size_hint();
    if let Some(progress) = &progress {
        progress(RegistrationProgressEvent::ArtifactStarted {
            artifact: entry.relative_path.clone(),
            total_bytes,
        });
    }
    let storage_progress = progress.map(|progress| {
        let artifact = entry.relative_path.clone();
        Arc::new(move |event: UploadProgress| {
            progress(RegistrationProgressEvent::ArtifactProgress {
                artifact: artifact.clone(),
                uploaded_bytes: event.uploaded_bytes,
                total_bytes: event.total_bytes,
            });
        }) as UploadProgressSink
    });
    storage
        .upload_artifact_with_progress(
            &entry.upload_id,
            &entry.plan,
            source,
            idempotency_key,
            storage_progress,
        )
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use wyrd_spec::error::WyrdError;
    use wyrd_spec::registry::RelativeArtifactPath;

    use super::validate_artifact_sources;

    fn path(value: &str) -> RelativeArtifactPath {
        RelativeArtifactPath::new(value).expect("test path is valid")
    }

    #[test]
    fn rejects_declared_source_without_server_plan() {
        let entries = Vec::new();
        let sources = BTreeMap::from([(path("weights.bin"), PathBuf::from("weights.bin"))]);

        let error: WyrdError = validate_artifact_sources(&entries, &sources)
            .expect_err("missing plan is rejected")
            .into();

        assert_eq!(error.code(), "WYRD_REGISTRY_400_UPLOAD_INTERRUPTED");
        assert!(error.as_problem_json().to_string().contains("weights.bin"));
    }

    #[test]
    fn rejects_server_plan_without_local_source() {
        let planned = BTreeSet::from([path("weights.bin")]);
        let entries = planned
            .into_iter()
            .map(|relative_path| wyrd_spec::registry::CardUploadEntry {
                upload_id: wyrd_spec::storage::UploadId::new(),
                relative_path,
                plan: wyrd_spec::storage::UploadPlan::LocalFs {
                    put_url: "http://localhost/upload".to_owned(),
                    ttl_secs: 60,
                },
            })
            .collect::<Vec<_>>();

        let error: WyrdError = validate_artifact_sources(&entries, &BTreeMap::new())
            .expect_err("unexpected plan is rejected")
            .into();

        assert_eq!(error.code(), "WYRD_REGISTRY_400_UPLOAD_INTERRUPTED");
        assert!(error.as_problem_json().to_string().contains("weights.bin"));
    }
}
