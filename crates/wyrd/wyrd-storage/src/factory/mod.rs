//! Backend store and signer construction shared by every storage consumer.
//!
//! [`build_signer`] builds the handle's signer, whose store is also the
//! handle's data plane; Bifrost builds its analytical store through
//! the [`crate::cloud::CloudStore`] constructors with [`client_options`], so artifact and
//! analytical storage share one client implementation per backend.

use std::time::Duration;

#[cfg(feature = "cloud")]
use crate::cloud::{CloudSigner, CloudStore};
use crate::error::StorageError;
use crate::local::LocalSigner;
use crate::settings::BackendConfig;
use crate::signer::BackendSigner;
#[cfg(feature = "cloud")]
use object_store::ClientOptions;

/// Idle-connection lifetime for a self-hosted or emulated object store
/// reached through a configured endpoint.
///
/// Such servers close idle keep-alive connections on their own schedule
/// (`RustFS` after about five seconds). A request written to a pooled
/// connection just as the server closes it fails with "connection closed
/// before message completed", and Wyrd deliberately never retries storage
/// writes behind the caller, so the pool drops idle connections well before
/// the shortest known server timeout.
const ENDPOINT_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(2);

/// Idle-connection lifetime for AWS S3, which closes idle keep-alive
/// connections after about 20 seconds.
#[cfg(feature = "cloud")]
const S3_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(15);

/// Idle-connection lifetime for managed GCS and Azure Blob Storage.
///
/// Both front ends keep idle connections for minutes (GCS about 610 seconds,
/// Azure 10 minutes), so the bound is the network path instead: 60 seconds
/// stays under the Azure outbound NAT's fixed 4-minute idle drop and the AWS
/// NAT gateway's 350 seconds while keeping connections hot between bursts.
#[cfg(feature = "cloud")]
const MANAGED_POOL_IDLE_TIMEOUT: Duration = Duration::from_mins(1);

/// Choose how long a storage HTTP client may reuse an idle connection.
///
/// Connections stay pooled as long as the storage server is known to keep
/// them, so steady Bifrost traffic reuses warm TLS connections, and are
/// dropped before the server closes them so no request races that close.
/// Any configured endpoint names a server whose timeout Wyrd does not know,
/// so it takes the shortest bound.
pub fn pool_idle_timeout(backend: &BackendConfig) -> Duration {
    match backend {
        BackendConfig::Local { .. } => ENDPOINT_POOL_IDLE_TIMEOUT,
        #[cfg(feature = "cloud")]
        BackendConfig::S3(c) if c.endpoint_url.is_none() => S3_POOL_IDLE_TIMEOUT,
        #[cfg(feature = "cloud")]
        BackendConfig::Gcs(c) if c.endpoint_url.is_none() => MANAGED_POOL_IDLE_TIMEOUT,
        #[cfg(feature = "cloud")]
        BackendConfig::Azure(c) if c.endpoint_url.is_none() => MANAGED_POOL_IDLE_TIMEOUT,
        _ => ENDPOINT_POOL_IDLE_TIMEOUT,
    }
}

/// Return the HTTP options every client for `backend` shares.
///
/// The idle-pool lifetime comes from [`pool_idle_timeout`]; plain HTTP is
/// allowed only for an `http://` endpoint (an emulator or local service).
/// Callers add their own request timeout.
#[cfg(feature = "cloud")]
#[must_use]
pub fn client_options(backend: &BackendConfig) -> ClientOptions {
    let endpoint = match backend {
        BackendConfig::Local { .. } => None,
        BackendConfig::S3(config) => config.endpoint_url.as_deref(),
        BackendConfig::Gcs(config) => config.endpoint_url.as_deref(),
        BackendConfig::Azure(config) => config.endpoint_url.as_deref(),
    };
    ClientOptions::new()
        .with_pool_idle_timeout(pool_idle_timeout(backend))
        .with_allow_http(endpoint.is_some_and(|endpoint| endpoint.starts_with("http://")))
}

/// Returns the service-account JSON from Wyrd's GCS credential variables.
///
/// `GOOGLE_ACCOUNT_JSON_BASE64` is decoded, else
/// `GOOGLE_APPLICATION_CREDENTIALS_JSON` is returned as is. `None` leaves
/// `object_store`'s default chain (a credentials file path or ambient
/// identity) in charge.
#[cfg(feature = "cloud")]
pub(crate) fn service_account_json() -> Option<String> {
    use base64::Engine as _;
    if let Ok(encoded) = std::env::var("GOOGLE_ACCOUNT_JSON_BASE64") {
        return base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()
            .and_then(|json| String::from_utf8(json).ok());
    }
    std::env::var("GOOGLE_APPLICATION_CREDENTIALS_JSON").ok()
}

/// Build the active backend signer in a cloud-enabled build.
///
/// A cloud signer owns the backend's one `object_store` client, built with no
/// request timeout and no retries because Wyrd never retries storage writes
/// behind the caller, and proves its credentials with one root listing.
///
/// # Errors
/// Returns [`StorageError::CryptoProvider`] when another Rustls provider
/// already owns the process, a backend error when the local root or the store
/// configuration is unusable, or [`StorageError::CredentialChain`] when the
/// boot probe fails. Cancellation interrupts the probe without constructing a
/// signer and makes no durable storage changes.
#[cfg(feature = "cloud")]
pub async fn build_signer(backend: &BackendConfig) -> Result<BackendSigner, StorageError> {
    wyrd_tls::install_crypto_provider()?;
    let retry = object_store::RetryConfig {
        max_retries: 0,
        ..object_store::RetryConfig::default()
    };
    let options = client_options(backend).with_timeout_disabled();
    let store = match backend {
        BackendConfig::Local { root } => {
            return Ok(BackendSigner::Local(LocalSigner::new(root.clone())?));
        }
        BackendConfig::S3(config) => CloudStore::s3(config, options, retry)?,
        BackendConfig::Gcs(config) => CloudStore::gcs(config, options, retry)?,
        BackendConfig::Azure(config) => CloudStore::azure(config, options, retry)?,
    };
    let http = reqwest::Client::builder()
        .pool_idle_timeout(pool_idle_timeout(backend))
        .build()
        .map_err(|error| StorageError::Backend {
            backend: backend.kind(),
            op: "build_signer",
            message: error.to_string(),
        })?;
    let signer = CloudSigner::new(store, http);
    signer.probe().await?;
    Ok(BackendSigner::Cloud(Box::new(signer)))
}

/// Build the active backend signer in a build without cloud support.
///
/// The returned immediately-ready future preserves the awaitable factory API
/// without creating an asynchronous state machine for local construction or a
/// deterministic cloud-capability error.
///
/// # Errors
/// Returns [`StorageError::CryptoProvider`] when another Rustls provider
/// already owns the process, a storage error when local signer construction
/// fails, or a cloud-capability error when a cloud backend is selected without
/// the `cloud` feature.
#[cfg(not(feature = "cloud"))]
pub fn build_signer(
    backend: &BackendConfig,
) -> std::future::Ready<Result<BackendSigner, StorageError>> {
    let result = wyrd_tls::install_crypto_provider()
        .map_err(StorageError::from)
        .and_then(|()| match backend {
            BackendConfig::Local { root } => LocalSigner::new(root.clone()).map(BackendSigner::Local),
            BackendConfig::S3(_) | BackendConfig::Gcs(_) | BackendConfig::Azure(_) => {
                Err(StorageError::Backend {
                    backend: backend.kind(),
                    op: "build_signer",
                    message: "cloud storage backends are not compiled in this build; rebuild with the `cloud` feature".to_owned(),
                })
            }
        });
    std::future::ready(result)
}

#[cfg(all(test, feature = "cloud"))]
mod tests {
    use super::*;

    /// Managed cloud stores keep warm connections for as long as each
    /// provider holds them; any configured endpoint takes the short bound
    /// because its server's keep-alive is unknown.
    ///
    /// # Panics
    /// Panics when any backend selects an idle bound other than the one its
    /// provider or configured endpoint requires.
    #[test]
    fn pool_idle_timeout_matches_the_storage_server() {
        use crate::settings::{AzureConfig, GcsConfig, S3Config};
        let endpoint = Some("http://127.0.0.1:9000".to_owned());
        let s3 = |endpoint_url| {
            BackendConfig::S3(S3Config {
                bucket: "b".to_owned(),
                region: None,
                endpoint_url,
            })
        };
        let gcs = |endpoint_url| {
            BackendConfig::Gcs(GcsConfig {
                bucket: "b".to_owned(),
                endpoint_url,
            })
        };
        let azure = |endpoint_url| {
            BackendConfig::Azure(AzureConfig {
                account: "a".to_owned(),
                container: "c".to_owned(),
                endpoint_url,
            })
        };

        assert_eq!(pool_idle_timeout(&s3(None)), S3_POOL_IDLE_TIMEOUT);
        assert_eq!(pool_idle_timeout(&gcs(None)), MANAGED_POOL_IDLE_TIMEOUT);
        assert_eq!(pool_idle_timeout(&azure(None)), MANAGED_POOL_IDLE_TIMEOUT);
        for backend in [s3(endpoint.clone()), gcs(endpoint.clone()), azure(endpoint)] {
            assert_eq!(pool_idle_timeout(&backend), ENDPOINT_POOL_IDLE_TIMEOUT);
        }
    }

    /// Wyrd's GCS credential variables yield the service-account JSON:
    /// the base64 variable decoded, else the JSON variable verbatim.
    #[test]
    fn service_account_json_reads_wyrd_gcs_variables() {
        use base64::Engine as _;
        let raw = r#"{"type":"service_account"}"#;
        let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
        let from_b64 = temp_env::with_vars(
            [
                ("GOOGLE_ACCOUNT_JSON_BASE64", Some(encoded.as_str())),
                ("GOOGLE_APPLICATION_CREDENTIALS_JSON", None),
            ],
            service_account_json,
        );
        let from_json = temp_env::with_vars(
            [
                ("GOOGLE_ACCOUNT_JSON_BASE64", None),
                ("GOOGLE_APPLICATION_CREDENTIALS_JSON", Some(raw)),
            ],
            service_account_json,
        );

        assert_eq!(from_b64.as_deref(), Some(raw));
        assert_eq!(from_json.as_deref(), Some(raw));
    }
}
