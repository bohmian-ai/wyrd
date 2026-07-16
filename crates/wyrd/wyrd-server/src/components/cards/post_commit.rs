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

    let uploads =
        initialize_manifest_batch(state, caller, &operation, &response, manifests).await?;
    response.uploads = uploads.clone();
    persist_upload_plans(state, caller, operation.operation_id, &uploads).await?;
    Ok(response)
}

/// Initialize each manifest independently so one bad artifact does not prevent
/// the remaining upload plans from being returned.
async fn initialize_manifest_batch(
    state: &AppState,
    caller: &Caller,
    operation: &CardRegistrationOperationRow,
    response: &CreateCardResponse,
    manifests: Vec<CardArtifactManifestRow>,
) -> Result<Vec<wyrd_spec::storage::UploadInitResponse>, WyrdError> {
    let mut uploads = Vec::new();
    for manifest in manifests {
        if let Some(upload) =
            initialize_manifest(state, caller, operation, response, manifest).await?
        {
            uploads.push(upload);
        }
    }
    Ok(uploads)
}

/// Create one storage upload initialization and mark its manifest as initialized.
///
/// Invalid sizes, invalid derived keys, and temporary storage failures are
/// logged and returned as `None`; the card remains pending so a later replay can
/// retry the missing initialization.
async fn initialize_manifest(
    state: &AppState,
    caller: &Caller,
    operation: &CardRegistrationOperationRow,
    response: &CreateCardResponse,
    manifest: CardArtifactManifestRow,
) -> Result<Option<wyrd_spec::storage::UploadInitResponse>, WyrdError> {
    let expected_size_bytes = match u64::try_from(manifest.expected_size_bytes) {
        Ok(size) => size,
        Err(error) => {
            tracing::warn!(path = %manifest.relative_path, error = %error, "skipping invalid artifact size");
            return Ok(None);
        }
    };
    let Some(idempotency_key) = artifact_idempotency_key(operation, &manifest.relative_path) else {
        return Ok(None);
    };
    let upload = match wyrd_storage::service::upload_init(
        &state.storage,
        state.postgres.wyrd(),
        &storage_caller(caller),
        Some(idempotency_key),
        UploadInitRequest {
            card_uid: response.card_uid.clone(),
            relative_path: manifest.relative_path.clone(),
            expected_sha256: manifest.expected_sha256.clone(),
            expected_size_bytes,
            content_type: manifest.content_type.clone(),
        },
    )
    .await
    {
        Ok(upload) => upload,
        Err(error) => {
            tracing::warn!(path = %manifest.relative_path, error = %error, "artifact upload initialization deferred");
            return Ok(None);
        }
    };
    mark_manifest_initialized(state, caller, response, &manifest, &upload).await?;
    Ok(Some(upload))
}

/// Derive the stable per-operation key used to initialize one artifact.
fn artifact_idempotency_key(
    operation: &CardRegistrationOperationRow,
    relative_path: &str,
) -> Option<wyrd_spec::ids::IdempotencyKey> {
    let key = format!(
        "wyrd-artifact-{}-{}",
        operation.operation_id,
        blake3::hash(relative_path.as_bytes()).to_hex()
    );
    match wyrd_spec::ids::IdempotencyKey::new(&key) {
        Ok(key) => Some(key),
        Err(error) => {
            tracing::warn!(path = relative_path, error = %error, "skipping invalid artifact idempotency key");
            None
        }
    }
}

/// Persist the storage upload ID on its manifest in a tenant transaction.
async fn mark_manifest_initialized(
    state: &AppState,
    caller: &Caller,
    response: &CreateCardResponse,
    manifest: &CardArtifactManifestRow,
    upload: &wyrd_spec::storage::UploadInitResponse,
) -> Result<(), WyrdError> {
    let upload_id = upload.upload_id.as_uuid().map_err(|error| {
        tracing::error!(error = %error, "storage returned an invalid upload id");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    mark_manifest_upload_initialized(
        &mut conn,
        &response.card_uid,
        &manifest.relative_path,
        upload_id,
    )
    .await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(())
}

/// Persist the complete replay inventory for an operation after initialization.
async fn persist_upload_plans(
    state: &AppState,
    caller: &Caller,
    operation_id: uuid::Uuid,
    uploads: &[wyrd_spec::storage::UploadInitResponse],
) -> Result<(), WyrdError> {
    let plans = mapping::upload_plans_json(uploads)?;
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    update_registration_operation_plans(
        &mut conn,
        RegistrationOperationId::new(operation_id),
        &plans,
    )
    .await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(())
}

/// Log a post-commit registry failure and return its stable public error.
fn registry_db_error(error: impl Display) -> WyrdError {
    tracing::error!(error = %error, "card registry database operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}
