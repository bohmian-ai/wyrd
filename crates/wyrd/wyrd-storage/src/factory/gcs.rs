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

/// Build a GCS signer pointed at a local emulator endpoint.
///
/// Uses anonymous credentials and a custom `storage_endpoint`. Set
/// `WYRD_STORAGE_INTEGRATION_GCS=1` and `WYRD_GCS_EMULATOR_HOST` in tests
/// to activate. Never call this in production.
///
/// # Errors
/// Returns an error when the bucket probe against the emulator fails.
#[cfg(any(test, feature = "emulator"))]
pub fn build_emulator_signer(bucket: &str, emulator_host: &str) -> Result<GcsSigner, StorageError> {
    let config = ClientConfig {
        storage_endpoint: emulator_host.trim_end_matches('/').to_owned(),
        ..ClientConfig::default().anonymous()
    };
    let client = Client::new(config);
    Ok(GcsSigner::new_emulator(
        client,
        bucket.to_owned(),
        emulator_host,
    ))
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
