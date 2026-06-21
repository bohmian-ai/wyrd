//! S3 backend factory.

use crate::error::StorageError;
use crate::s3::S3Signer;
use crate::settings::S3Config;
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Builder, Region};
use opendal::services;

pub(crate) fn s3_service(cfg: &S3Config) -> services::S3 {
    let mut b = services::S3::default().bucket(&cfg.bucket);
    if let Some(r) = &cfg.region {
        b = b.region(r);
    }
    if let Some(e) = &cfg.endpoint_url {
        b = b.endpoint(e);
    }
    // object_store defaulted to virtual-hosted; opendal defaults to path-style.
    // Preserve prior behavior: virtual-host unless force_path_style or custom endpoint.
    if !cfg.force_path_style && cfg.endpoint_url.is_none() {
        b = b.enable_virtual_host_style();
    }
    b
}

/// Build the S3 signer from the AWS default credential chain.
///
/// # Errors
/// Returns an error when the SDK probe cannot access the configured bucket.
pub async fn build_signer(config: &S3Config) -> Result<S3Signer, StorageError> {
    let shared = aws_config::defaults(BehaviorVersion::latest()).load().await;
    let mut builder = Builder::from(&shared);
    if let Some(region) = &config.region {
        builder = builder.region(Region::new(region.clone()));
    }
    if let Some(endpoint_url) = &config.endpoint_url {
        builder = builder.endpoint_url(endpoint_url.clone());
    }
    if config.force_path_style || config.endpoint_url.is_some() {
        builder = builder.force_path_style(true);
    }
    let client = Client::from_conf(builder.build());
    client
        .head_bucket()
        .bucket(config.bucket.as_str())
        .send()
        .await
        .map_err(|source| {
            tracing::error!(
                bucket = %config.bucket,
                error = ?source,
                "S3 boot probe failed; check credentials and bucket permissions"
            );
            StorageError::CredentialChain("s3")
        })?;

    Ok(S3Signer::new(client, config.bucket.clone()))
}

/// Build an S3 signer pointed at a local emulator (`RustFS` / `MinIO`).
///
/// Uses static test credentials (overridable via `WYRD_S3_EMULATOR_ACCESS_KEY`
/// / `WYRD_S3_EMULATOR_SECRET_KEY`) and path-style addressing. Never call this
/// in production.
///
/// # Errors
/// Infallible today; returns `Result` for symmetry with the GCS/Azure emulator
/// signer builders.
#[cfg(any(test, feature = "emulator"))]
pub fn build_emulator_signer(bucket: &str, endpoint: &str) -> Result<S3Signer, StorageError> {
    use aws_sdk_s3::config::{Builder, Credentials, Region};

    let access =
        std::env::var("WYRD_S3_EMULATOR_ACCESS_KEY").unwrap_or_else(|_| "wyrd-test-key".to_owned());
    let secret = std::env::var("WYRD_S3_EMULATOR_SECRET_KEY")
        .unwrap_or_else(|_| "wyrd-test-secret".to_owned());
    let region =
        std::env::var("WYRD_STORAGE_S3_REGION").unwrap_or_else(|_| "us-east-1".to_owned());
    let conf = Builder::default()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new(region))
        .endpoint_url(endpoint)
        .force_path_style(true)
        .credentials_provider(Credentials::new(access, secret, None, None, "static"))
        .build();
    Ok(S3Signer::new(Client::from_conf(conf), bucket.to_owned()))
}
