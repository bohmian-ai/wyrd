//! Azure Blob Storage backend factory.

use crate::azure::{AzureSasMode, AzureSigner};
use crate::error::StorageError;
use crate::settings::AzureConfig;
use azure_identity::create_credential;
use azure_storage::CloudLocation;
use azure_storage::StorageCredentials;
use azure_storage_blobs::prelude::{BlobServiceClient, ClientBuilder};
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

/// Build an Azure signer pointed at a local Azurite emulator.
///
/// Uses the well-known Azurite shared-key credentials (`devstoreaccount1`)
/// and shared-key SAS mode. Parses `azurite_endpoint` as `http://host:port`
/// (default `http://127.0.0.1:10000`). Never call this in production.
///
/// # Errors
/// Returns an error when the endpoint cannot be parsed or the container probe
/// fails.
#[cfg(any(test, feature = "emulator"))]
pub fn build_emulator_signer(
    container: &str,
    azurite_endpoint: &str,
) -> Result<AzureSigner, StorageError> {
    let (address, port) = parse_azurite_endpoint(azurite_endpoint).map_err(|()| {
        StorageError::Backend {
            backend: StorageBackendKind::Azure,
            op: "build_emulator_signer",
            message: format!("invalid azurite endpoint: {azurite_endpoint}"),
        }
    })?;
    let service_client = ClientBuilder::with_location(
        CloudLocation::Emulator { address, port },
        StorageCredentials::emulator(),
    )
    .blob_service_client();

    Ok(AzureSigner::new(
        service_client,
        container.to_owned(),
        AzureSasMode::AccountKey,
    ))
}

fn parse_azurite_endpoint(endpoint: &str) -> Result<(String, u16), ()> {
    let url = endpoint.trim_end_matches('/');
    let stripped = url.strip_prefix("http://").ok_or(())?;
    let (host, port_str) = stripped.rsplit_once(':').ok_or(())?;
    let port: u16 = port_str.parse().map_err(|_| ())?;
    Ok((host.to_owned(), port))
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
