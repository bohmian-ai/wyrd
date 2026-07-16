//! Post-commit artifact initialization and replay-plan persistence.

use std::fmt::Display;

use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{CardLifecycleStatus, CreateCardResponse, RegistrationOperationId};
use wyrd_spec::storage::UploadInitRequest;
use wyrd_sql::queries::cards::{
    CardArtifactManifestRow, CardRegistrationOperationRow, mark_manifest_upload_initialized,
    update_registration_operation_plans,
};

use crate::components::auth::Caller;
use crate::components::cards::mapping;
use crate::components::storage::routes::storage_caller;
use crate::state::AppState;

/// Initialize deferred artifact uploads after the registry transaction commits.
pub(crate) async fn drive_post_commit_inits(
    state: &AppState,
    caller: &Caller,
    operation: CardRegistrationOperationRow,
    mut response: CreateCardResponse,
    manifests: Vec<CardArtifactManifestRow>,
) -> Result<CreateCardResponse, WyrdError> {
    if response.status == CardLifecycleStatus::Active || manifests.is_empty() {
        return Ok(response);
    }

    let mut uploads = Vec::new();
    for manifest in manifests {
        let expected_size_bytes = match u64::try_from(manifest.expected_size_bytes) {
            Ok(size) => size,
            Err(error) => {
                tracing::warn!(path = %manifest.relative_path, error = %error, "skipping invalid artifact size");
                continue;
            }
        };
        let key = format!(
            "wyrd-artifact-{}-{}",
            operation.operation_id,
            blake3::hash(manifest.relative_path.as_bytes()).to_hex()
        );
        let idempotency_key = match wyrd_spec::ids::IdempotencyKey::new(&key) {
            Ok(key) => key,
            Err(error) => {
                tracing::warn!(path = %manifest.relative_path, error = %error, "skipping invalid artifact idempotency key");
                continue;
            }
        };
        let result = wyrd_storage::service::upload_init(
            &state.storage,
            state.postgres.wyrd(),
            &storage_caller(caller),
            Some(idempotency_key),
            UploadInitRequest {
                card_uid: response.card_uid.clone(),
                relative_path: manifest.relative_path.clone(),
                expected_sha256: manifest.expected_sha256,
                expected_size_bytes,
                content_type: manifest.content_type,
            },
        )
        .await;
        let upload = match result {
            Ok(upload) => upload,
            Err(error) => {
                tracing::warn!(path = %manifest.relative_path, error = %error, "artifact upload initialization deferred");
                continue;
            }
        };
        let mut conn = state
            .postgres
            .tenant_conn(caller.data_tenant_id)
            .await
            .map_err(registry_db_error)?;
        mark_manifest_upload_initialized(
            &mut conn,
            &response.card_uid,
            &manifest.relative_path,
            upload.upload_id.as_uuid().map_err(|error| {
                tracing::error!(error = %error, "storage returned an invalid upload id");
                WyrdError::registry_unavailable("card registry unavailable")
            })?,
        )
        .await?;
        conn.commit().await.map_err(registry_db_error)?;
        uploads.push(upload);
    }
    response.uploads = uploads.clone();

    let plans = mapping::upload_plans_json(&uploads)?;
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    update_registration_operation_plans(
        &mut conn,
        RegistrationOperationId::new(operation.operation_id),
        &plans,
    )
    .await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(response)
}

fn registry_db_error(error: impl Display) -> WyrdError {
    tracing::error!(error = %error, "card registry database operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}
