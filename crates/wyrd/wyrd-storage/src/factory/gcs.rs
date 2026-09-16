//! Google Cloud Storage backend factory.

use crate::error::StorageError;
use crate::gcs::GcsSigner;
use crate::settings::GcsConfig;
use base64::{Engine, engine::general_purpose::STANDARD};
use gcloud_storage::client::{Client, ClientConfig};
use opendal::services;

fn credential_for_gcs() -> Option<String> {
    if let Ok(b64) = std::env::var("GOOGLE_ACCOUNT_JSON_BASE64") {
        // Pass the base64 string through verbatim — opendal calls from_base64 internally.
        // Do NOT decode here.
        Some(b64)
    } else if let Ok(json) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS_JSON") {
        Some(STANDARD.encode(&json))
    } else {
        None
    }
}

pub(crate) fn gcs_service(cfg: &GcsConfig) -> services::Gcs {
    let mut b = services::Gcs::default().bucket(&cfg.bucket);
    let mut has_credential = false;
    if let Some(cred) = credential_for_gcs() {
        b = b.credential(&cred);
        has_credential = true;
    } else if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        b = b.credential_path(&path);
        has_credential = true;
    }
    // else: opendal's default chain = full ADC (env SA path, gcloud well-known file,
    // GCE/GKE metadata server, Workload Identity, WIF external_account).
    if let Some(endpoint) = &cfg.endpoint_url {
        b = b.endpoint(endpoint);
        // Anonymous GCS-compatible emulators (fake-gcs) reject signed requests.
        if !has_credential {
            b = b.skip_signature();
        }
    }
    b
}

/// Build the GCS signer using the Wyrd credential cascade.
///
/// A configured endpoint selects the anonymous emulator signer in builds with
/// the `emulator` feature. Without that feature an endpoint is refused rather
/// than silently signing against the real provider.
///
/// # Errors
/// Returns an error when an endpoint is configured without the `emulator`
/// feature, or when credentials or bucket access fail.
pub async fn build_signer(config: &GcsConfig) -> Result<GcsSigner, StorageError> {
    #[cfg(feature = "emulator")]
    if let Some(endpoint) = &config.endpoint_url {
        return Ok(emulator_signer(&config.bucket, endpoint));
    }
    #[cfg(not(feature = "emulator"))]
    if config.endpoint_url.is_some() {
        return Err(super::endpoint_requires_emulator(
            wyrd_spec::storage::StorageBackendKind::Gcs,
        ));
    }
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

/// Build a GCS signer for a `fake-gcs-server` emulator at `endpoint`.
///
/// Uses anonymous credentials and the emulator's unsigned media URLs, so it
/// performs no IO and no boot probe.
#[cfg(feature = "emulator")]
fn emulator_signer(bucket: &str, endpoint: &str) -> GcsSigner {
    let config = ClientConfig {
        storage_endpoint: endpoint.trim_end_matches('/').to_owned(),
        ..ClientConfig::default().anonymous()
    };
    GcsSigner::new_emulator(Client::new(config), bucket.to_owned(), endpoint)
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
    if let Ok(json) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS_JSON") {
        let credentials = CredentialsFile::new_from_str(&json).await?;
        return ClientConfig::default().with_credentials(credentials).await;
    }
    if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
        let credentials = CredentialsFile::new().await?;
        return ClientConfig::default().with_credentials(credentials).await;
    }

    ClientConfig::default().with_auth().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_env_passed_through_verbatim() {
        let encoded = STANDARD.encode(b"{\"type\":\"service_account\"}");
        let vars: Vec<(&str, Option<&str>)> = vec![
            ("GOOGLE_ACCOUNT_JSON_BASE64", Some(&encoded)),
            ("GOOGLE_APPLICATION_CREDENTIALS_JSON", None),
        ];
        let got = temp_env::with_vars(vars, credential_for_gcs);
        assert_eq!(got.expect("b64 var should produce Some"), encoded);
    }

    #[test]
    fn json_env_encoded_before_passing() {
        let raw = r#"{"type":"service_account"}"#;
        let expected = STANDARD.encode(raw);
        let vars: Vec<(&str, Option<&str>)> = vec![
            ("GOOGLE_ACCOUNT_JSON_BASE64", None),
            ("GOOGLE_APPLICATION_CREDENTIALS_JSON", Some(raw)),
        ];
        let got = temp_env::with_vars(vars, credential_for_gcs);
        assert_eq!(got.expect("json var should produce Some"), expected);
    }
}
