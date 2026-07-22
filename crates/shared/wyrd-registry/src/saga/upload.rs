//! Bounded artifact transfer through the shared storage dispatcher.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use futures_util::stream::{self, StreamExt};
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{
    CardUploadEntry, CardUploadPlan, CreateCardResponse, RelativeArtifactPath,
};
use wyrd_spec::storage::UploadPlan;
use wyrd_storage_client::{
    FileSource, PartUrlMinter, UploadHooks, UploadOutcome, WyrdStorageClient,
};

use crate::error::RegistryEngineError;

/// Transfer every server-minted artifact plan with bounded concurrency.
pub(crate) async fn artifacts(
    storage: &WyrdStorageClient,
    response: &CreateCardResponse,
    sources: &BTreeMap<RelativeArtifactPath, PathBuf>,
    idempotency_key: &str,
) -> Result<(), RegistryEngineError> {
    let plans = response
        .upload_plans
        .iter()
        .map(|plan| (plan, plan_paths(plan)))
        .collect::<Vec<_>>();
    let planned = plans
        .iter()
        .flat_map(|(_, paths)| paths.iter().cloned())
        .collect::<BTreeSet<_>>();
    validate_path_sets(&planned, sources)?;

    let uploads = plans
        .into_iter()
        .flat_map(|(plan, _)| plan.entries.iter().cloned())
        .filter_map(|entry| {
            sources
                .get(&entry.relative_path)
                .cloned()
                .map(|source| (entry, source))
        })
        .collect::<Vec<_>>();
    let results = stream::iter(uploads)
        .map(|(entry, source)| {
            let storage = storage.clone();
            let idempotency_key = idempotency_key.to_owned();
            async move { upload_one(&storage, &entry, source, &idempotency_key).await }
        })
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;
    for result in results {
        result?;
    }
    Ok(())
}

fn validate_path_sets(
    planned: &BTreeSet<RelativeArtifactPath>,
    sources: &BTreeMap<RelativeArtifactPath, PathBuf>,
) -> Result<(), RegistryEngineError> {
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

fn plan_paths(plan: &CardUploadPlan) -> BTreeSet<RelativeArtifactPath> {
    plan.entries
        .iter()
        .map(|entry| entry.relative_path.clone())
        .collect()
}

async fn upload_one(
    storage: &WyrdStorageClient,
    entry: &CardUploadEntry,
    source: PathBuf,
    idempotency_key: &str,
) -> Result<(), RegistryEngineError> {
    let source = FileSource::open(source).await?;
    let part_url_minter = s3_part_url_minter(storage, entry);
    let outcome = storage
        .upload(
            &entry.plan,
            source,
            UploadHooks {
                idempotency_key,
                progress: None,
                part_url_minter,
            },
        )
        .await?;
    if let UploadOutcome::NeedsServerComplete(request) = outcome {
        storage.complete(&entry.upload_id, &request).await?;
    }
    Ok(())
}

/// Build the authenticated S3 part-URL callback for one server-owned upload.
fn s3_part_url_minter<'a>(
    storage: &WyrdStorageClient,
    entry: &CardUploadEntry,
) -> Option<PartUrlMinter<'a>> {
    if !matches!(&entry.plan, UploadPlan::S3Multipart { .. }) {
        return None;
    }
    let storage = storage.clone();
    let upload_id = entry.upload_id.clone();
    Some(Box::new(move |part_number| {
        let storage = storage.clone();
        let upload_id = upload_id.clone();
        Box::pin(async move { storage.part_url(&upload_id, part_number).await })
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use wyrd_spec::error::WyrdError;
    use wyrd_spec::registry::RelativeArtifactPath;

    use super::validate_path_sets;

    fn path(value: &str) -> RelativeArtifactPath {
        RelativeArtifactPath::new(value).expect("test path is valid")
    }

    #[test]
    fn rejects_declared_source_without_server_plan() {
        let planned = BTreeSet::new();
        let sources = BTreeMap::from([(path("weights.bin"), PathBuf::from("weights.bin"))]);

        let error: WyrdError = validate_path_sets(&planned, &sources)
            .expect_err("missing plan is rejected")
            .into();

        assert_eq!(error.code(), "WYRD_REGISTRY_400_UPLOAD_INTERRUPTED");
        assert!(error.as_problem_json().to_string().contains("weights.bin"));
    }

    #[test]
    fn rejects_server_plan_without_local_source() {
        let planned = BTreeSet::from([path("weights.bin")]);
        let sources = BTreeMap::new();

        let error: WyrdError = validate_path_sets(&planned, &sources)
            .expect_err("unexpected plan is rejected")
            .into();

        assert_eq!(error.code(), "WYRD_REGISTRY_400_UPLOAD_INTERRUPTED");
        assert!(error.as_problem_json().to_string().contains("weights.bin"));
    }
}
