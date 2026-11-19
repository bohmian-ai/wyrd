//! GCP Cloud KMS wrapping: `cryptoKeys.encrypt` and `cryptoKeys.decrypt`.
//!
//! The configured reference is a CryptoKey resource name. Encrypt uses its
//! primary version; Decrypt lets KMS select the version the ciphertext needs,
//! so native rotation needs no Wyrd change. The canonical wrap context is
//! sent as `additionalAuthenticatedData`. The access token comes from the
//! standard GCP credential chain (service account file, application default
//! credentials, or the metadata server), refreshed by `object_store`'s
//! provider before it expires.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use object_store::gcp::{GcpCredentialProvider, GoogleCloudStorageBuilder};
use reqwest::{Client, StatusCode};
use serde_json::json;
use url::Url;
use zeroize::Zeroizing;

use crate::remote::{self, unavailable};
use crate::{KmsBackendKind, KmsError, ProviderFailure};

/// This backend's kind.
const KIND: KmsBackendKind = KmsBackendKind::GcpKms;

/// Public Cloud KMS API base.
const DEFAULT_ENDPOINT: &str = "https://cloudkms.googleapis.com";

/// A reusable authenticated Cloud KMS client.
pub(crate) struct GcpKms {
    /// Bounded HTTP client.
    client: Client,
    /// API base URL.
    endpoint: String,
    /// Workload credential chain.
    credentials: GcpCredentialProvider,
}

impl GcpKms {
    /// Build over the ambient GCP credential chain.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] when the credential chain or client
    /// cannot be built.
    pub(crate) fn new(endpoint: Option<&Url>) -> Result<Self, KmsError> {
        let store = GoogleCloudStorageBuilder::from_env()
            .with_bucket_name("wyrd-kms")
            .build()
            .map_err(|_| KmsError::Config {
                setting: "backend",
                reason: "could not load GCP workload credentials",
            })?;
        Self::with_credentials(
            endpoint.map_or(DEFAULT_ENDPOINT, Url::as_str),
            store.credentials().clone(),
            remote::CALL_TIMEOUT,
        )
    }

    /// Build over explicit `credentials` and `timeout`.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] when the client cannot be built.
    pub(crate) fn with_credentials(
        endpoint: &str,
        credentials: GcpCredentialProvider,
        timeout: Duration,
    ) -> Result<Self, KmsError> {
        Ok(Self {
            client: remote::client(timeout)?,
            endpoint: endpoint.trim_end_matches('/').to_owned(),
            credentials,
        })
    }

    /// Encrypt `dek` under CryptoKey `key`, authenticating `aad`.
    ///
    /// # Errors
    /// Returns [`KmsError::Unavailable`] for a credential, transport,
    /// timeout, or refusal failure.
    pub(crate) async fn wrap(
        &self,
        key: &str,
        dek: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, KmsError> {
        let answer = self
            .call(
                key,
                "encrypt",
                &json!({
                    "plaintext": STANDARD.encode(dek),
                    "additionalAuthenticatedData": STANDARD.encode(aad),
                }),
                false,
            )
            .await?;
        remote::base64_field(KIND, &answer, "ciphertext").map(|wrapped| wrapped.to_vec())
    }

    /// Decrypt `wrapped` under CryptoKey `key`, requiring `aad`.
    ///
    /// # Errors
    /// Returns [`KmsError::Authentication`] when KMS rejects the ciphertext
    /// or context, and otherwise [`KmsError::Unavailable`].
    pub(crate) async fn unwrap(
        &self,
        key: &str,
        wrapped: &[u8],
        aad: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, KmsError> {
        let answer = self
            .call(
                key,
                "decrypt",
                &json!({
                    "ciphertext": STANDARD.encode(wrapped),
                    "additionalAuthenticatedData": STANDARD.encode(aad),
                }),
                true,
            )
            .await?;
        remote::base64_field(KIND, &answer, "plaintext")
    }

    /// POST `body` to `{endpoint}/v1/{key}:{method}` with a fresh bearer
    /// token and map the status.
    ///
    /// # Errors
    /// Returns [`KmsError::Authentication`] for a 400 on decrypt and
    /// [`KmsError::Unavailable`] otherwise.
    async fn call(
        &self,
        key: &str,
        method: &str,
        body: &serde_json::Value,
        decrypting: bool,
    ) -> Result<serde_json::Value, KmsError> {
        let token = self
            .credentials
            .get_credential()
            .await
            .map_err(|_| unavailable(KIND, ProviderFailure::Credential))?;
        let request = self
            .client
            .post(format!("{}/v1/{key}:{method}", self.endpoint))
            .bearer_auth(&token.bearer)
            .json(body);
        let (status, answer) = remote::send(KIND, request).await?;
        match status {
            StatusCode::OK => Ok(answer),
            StatusCode::BAD_REQUEST if decrypting => Err(KmsError::Authentication),
            StatusCode::BAD_REQUEST
            | StatusCode::UNAUTHORIZED
            | StatusCode::FORBIDDEN
            | StatusCode::NOT_FOUND => Err(unavailable(KIND, ProviderFailure::Refused)),
            _ => Err(unavailable(KIND, ProviderFailure::Transport)),
        }
    }
}
