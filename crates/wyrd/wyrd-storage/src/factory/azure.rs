//! Azure Blob Storage backend factory.

use crate::azure::{AzureSasMode, AzureSigner};
use crate::error::StorageError;
use crate::settings::AzureConfig;
use azure_identity::create_credential;
#[cfg(feature = "emulator")]
use azure_storage::CloudLocation;
use azure_storage::StorageCredentials;
use azure_storage_blobs::prelude::BlobServiceClient;
#[cfg(feature = "emulator")]
use azure_storage_blobs::prelude::ClientBuilder;
use opendal::services;

pub(crate) fn azblob_service(cfg: &AzureConfig) -> services::Azblob {
    let endpoint = cfg
        .endpoint_url
        .clone()
        .unwrap_or_else(|| format!("https://{}.blob.core.windows.net", cfg.account));
    let b = services::Azblob::default()
        .account_name(&cfg.account)
        .container(&cfg.container)
        .endpoint(&endpoint);
    #[cfg(any(test, feature = "emulator"))]
    let b = if let Ok(key) = std::env::var("AZURE_STORAGE_ACCOUNT_KEY") {
        b.account_key(&key)
    } else {
        b
    };
    b
}

/// Build the Azure signer from Azure's default credential chain.
///
/// A configured endpoint selects the Azurite emulator signer in builds with
/// the `emulator` feature. Without that feature an endpoint is refused rather
/// than silently signing against the real provider.
///
/// # Errors
/// Returns an error when an endpoint is configured without the `emulator`
/// feature, or when credentials or container access fail.
pub async fn build_signer(config: &AzureConfig) -> Result<AzureSigner, StorageError> {
    #[cfg(feature = "emulator")]
    if let Some(endpoint) = &config.endpoint_url {
        return Ok(emulator_signer(config, endpoint));
    }
    #[cfg(not(feature = "emulator"))]
    if config.endpoint_url.is_some() {
        return Err(super::endpoint_requires_emulator(
            wyrd_spec::storage::StorageBackendKind::Azure,
        ));
    }
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

/// Build an Azure signer for an Azurite emulator.
///
/// `endpoint` is the complete service URL including the account, for example
/// `http://127.0.0.1:10000/devstoreaccount1`. Uses the well-known emulator
/// shared-key credentials and account-key SAS; performs no IO.
#[cfg(feature = "emulator")]
fn emulator_signer(config: &AzureConfig, endpoint: &str) -> AzureSigner {
    let service_client = ClientBuilder::with_location(
        CloudLocation::Custom {
            account: config.account.clone(),
            uri: endpoint.trim_end_matches('/').to_owned(),
        },
        StorageCredentials::emulator(),
    )
    .blob_service_client();
    AzureSigner::new(
        service_client,
        config.container.clone(),
        AzureSasMode::AccountKey,
    )
}
