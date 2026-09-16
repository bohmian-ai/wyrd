//! S3 backend factory.

use crate::error::StorageError;
use crate::s3::S3Signer;
use crate::settings::S3Config;
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Builder, Region};
use opendal::services;

/// Build the `OpenDAL` S3 service for the data-plane operator.
///
/// Without an endpoint the service uses AWS virtual-hosted addressing; a
/// custom endpoint (an S3-compatible service or emulator) keeps `OpenDAL`'s
/// path-style default.
pub(crate) fn s3_service(cfg: &S3Config) -> services::S3 {
    let mut b = services::S3::default().bucket(&cfg.bucket);
    if let Some(r) = &cfg.region {
        b = b.region(r);
    }
    match &cfg.endpoint_url {
        Some(e) => b.endpoint(e),
        None => b.enable_virtual_host_style(),
    }
}

/// Build the S3 signer from the AWS default credential chain.
///
/// A configured endpoint addresses an S3-compatible service with path-style
/// requests; otherwise the SDK's provider-default addressing applies.
///
/// # Errors
/// Returns [`StorageError::CryptoProvider`] when another Rustls provider
/// already owns the process. Other storage errors report failure to load AWS
/// credentials or access the configured bucket. Cancellation can leave the
/// read-only bucket probe outcome unknown but makes no durable storage changes.
///
pub async fn build_signer(config: &S3Config) -> Result<S3Signer, StorageError> {
    wyrd_tls::install_crypto_provider()?;
    let shared = aws_config::defaults(BehaviorVersion::latest()).load().await;
    let mut builder = Builder::from(&shared);
    if let Some(region) = &config.region {
        builder = builder.region(Region::new(region.clone()));
    }
    if let Some(endpoint_url) = &config.endpoint_url {
        builder = builder
            .endpoint_url(endpoint_url.clone())
            .force_path_style(true);
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
