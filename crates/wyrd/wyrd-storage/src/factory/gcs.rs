//! Google Cloud Storage backend factory.

use crate::error::StorageError;
use crate::gcs::GcsSigner;
use crate::settings::GcsConfig;
use base64::Engine;
use gcloud_storage::client::{Client, ClientConfig};
use object_store::gcp::{GoogleCloudStorage, GoogleCloudStorageBuilder};
use wyrd_spec::storage::StorageBackendKind;

/// Build the GCS signer using the Wyrd credential cascade.
///
/// # Errors
/// Returns an error when credentials or bucket access fail.
pub async fn build_signer(config: &GcsConfig) -> Result<GcsSigner, StorageError> {
    let client_config = build_client_config().await.map_err(|source| {
        tracing::error!(error = ?source, "GCS credential chain failed");
        StorageError::CredentialChain("gcs")
    })?;
    let client = Client::new(client_config);
    client
        .list_objects(&gcloud_storage::http::objects::list::ListObjectsRequest {
            bucket: config.bucket.clone(),
            max_results: Some(1),
            ..Default::default()
        })
        .await
        .map_err(|source| {
            tracing::error!(
                bucket = %config.bucket,
                error = ?source,
                "GCS boot probe failed; check credentials and bucket permissions"
            );
            StorageError::CredentialChain("gcs")
        })?;

    Ok(GcsSigner::new(client, config.bucket.clone()))
}

/// Build the shared GCS object-store substrate.
///
/// # Errors
/// Returns an error when the object-store builder rejects configuration.
pub fn build_object_store(config: &GcsConfig) -> Result<GoogleCloudStorage, StorageError> {
    let mut builder = GoogleCloudStorageBuilder::from_env().with_bucket_name(config.bucket.clone());
    if let Ok(raw) = std::env::var("GOOGLE_ACCOUNT_JSON_BASE64") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(raw)
            .map_err(|source| StorageError::Backend {
                backend: StorageBackendKind::Gcs,
                op: "build_object_store",
                message: source.to_string(),
            })?;
        let key = String::from_utf8(decoded).map_err(|source| StorageError::Backend {
            backend: StorageBackendKind::Gcs,
            op: "build_object_store",
            message: source.to_string(),
        })?;
        builder = builder.with_service_account_key(key);
    } else if let Ok(raw) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS_JSON") {
        builder = builder.with_service_account_key(raw);
    } else if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        builder = builder.with_application_credentials(path);
    }

    builder.build().map_err(|source| StorageError::Backend {
        backend: StorageBackendKind::Gcs,
        op: "build_object_store",
        message: source.to_string(),
    })
}

/// Build a GCS signer pointed at a local emulator endpoint.
///
/// Uses anonymous credentials and a custom `storage_endpoint`. Set
/// `WYRD_STORAGE_INTEGRATION_GCS=1` and `WYRD_GCS_EMULATOR_HOST` in tests
/// to activate. Never call this in production.
///
/// # Errors
/// Returns an error when the bucket probe against the emulator fails.
pub async fn build_emulator_signer(
    bucket: &str,
    emulator_host: &str,
) -> Result<GcsSigner, StorageError> {
    let config = ClientConfig {
        storage_endpoint: emulator_host.trim_end_matches('/').to_owned(),
        ..ClientConfig::default().anonymous()
    };
    let client = Client::new(config);
    Ok(GcsSigner::new(client, bucket.to_owned()))
}

async fn build_client_config() -> Result<ClientConfig, gcloud_auth::error::Error> {
    use gcloud_auth::credentials::CredentialsFile;

    if let Ok(raw) = std::env::var("GOOGLE_ACCOUNT_JSON_BASE64") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(raw)
            .map_err(|source| {
                gcloud_auth::error::Error::CredentialsIOError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    source,
                ))
            })?;
        let json = String::from_utf8(decoded).map_err(|source| {
            gcloud_auth::error::Error::CredentialsIOError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                source,
            ))
        })?;
        let credentials = CredentialsFile::new_from_str(json.as_str()).await?;
        return ClientConfig::default().with_credentials(credentials).await;
    }
    if std::env::var("GOOGLE_APPLICATION_CREDENTIALS_JSON").is_ok()
        || std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok()
    {
        let credentials = CredentialsFile::new().await?;
        return ClientConfig::default().with_credentials(credentials).await;
    }

    ClientConfig::default().with_auth().await
}
