//! Azure Blob Storage backend factory.

use crate::azure::{AzureSasMode, AzureSigner};
use crate::error::StorageError;
use crate::settings::AzureConfig;
use azure_identity::create_credential;
use azure_storage::StorageCredentials;
use azure_storage_blobs::prelude::BlobServiceClient;
use object_store::azure::{MicrosoftAzure, MicrosoftAzureBuilder};
use wyrd_spec::storage::StorageBackendKind;

/// Build the Azure signer from Azure's default credential chain.
///
/// # Errors
/// Returns an error when credentials or container access fail.
pub async fn build_signer(config: &AzureConfig) -> Result<AzureSigner, StorageError> {
    let credential = create_credential().map_err(|source| {
        tracing::error!(error = ?source, "Azure credential chain failed");
        StorageError::CredentialChain("azure")
    })?;
    let credentials = StorageCredentials::token_credential(credential);
    let service_client = BlobServiceClient::new(config.account.clone(), credentials);
    service_client
        .container_client(config.container.as_str())
        .get_properties()
        .await
        .map_err(|source| {
            tracing::error!(
                account = %config.account,
                container = %config.container,
                error = ?source,
                "Azure boot probe failed; check credentials and container permissions"
            );
            StorageError::CredentialChain("azure")
        })?;

    Ok(AzureSigner::new(
        service_client,
        config.container.clone(),
        AzureSasMode::UserDelegation,
    ))
}

/// Build the shared Azure object-store substrate.
///
/// # Errors
/// Returns an error when the object-store builder rejects configuration.
pub fn build_object_store(config: &AzureConfig) -> Result<MicrosoftAzure, StorageError> {
    let builder = MicrosoftAzureBuilder::from_env()
        .with_account(config.account.clone())
        .with_container_name(config.container.clone());
    builder.build().map_err(|source| StorageError::Backend {
        backend: StorageBackendKind::Azure,
        op: "build_object_store",
        message: source.to_string(),
    })
}
