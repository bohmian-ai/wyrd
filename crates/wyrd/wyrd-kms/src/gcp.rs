//! GCP Cloud KMS wrapping: `cryptoKeys.encrypt` and `cryptoKeys.decrypt`.
//!
//! The configured reference is a CryptoKey resource name. Encrypt uses its
//! primary version; Decrypt lets KMS select the version the ciphertext needs,
//! so native rotation needs no Wyrd change. The canonical wrap context is
//! sent as `additionalAuthenticatedData`. The access token comes from the
//! same identity Google Cloud Storage selects: Wyrd's inline service-account
//! variables when set, else the standard GCP credential chain (service
//! account file, application default credentials, or the metadata server),
//! refreshed by `object_store`'s provider before it expires.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use object_store::gcp::{GcpCredentialProvider, GoogleCloudStorageBuilder};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use url::Url;
use wyrd_crypt::SecretBytes;
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
    /// Build over the service account Wyrd's inline Google variables name,
    /// else the ambient GCP credential chain.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] when the credential chain or client
    /// cannot be built.
    pub(crate) fn new(endpoint: Option<&Url>) -> Result<Self, KmsError> {
        let mut builder = GoogleCloudStorageBuilder::from_env().with_bucket_name("wyrd-kms");
        if let Some(key) = wyrd_utils::secret::google_service_account_json() {
            builder = builder.with_service_account_key(key);
        }
        let store = builder.build().map_err(|_| KmsError::Config {
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
        let plaintext = Zeroizing::new(STANDARD.encode(dek));
        let body = remote::secret_json(&Request {
            plaintext: Some(&plaintext),
            ciphertext: None,
            additional_authenticated_data: &STANDARD.encode(aad),
        })?;
        let answer = self.call(key, "encrypt", body, false).await?;
        remote::base64_field(KIND, &answer, "ciphertext")
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
    ) -> Result<SecretBytes, KmsError> {
        let body = remote::secret_json(&Request {
            plaintext: None,
            ciphertext: Some(&STANDARD.encode(wrapped)),
            additional_authenticated_data: &STANDARD.encode(aad),
        })?;
        let answer = self.call(key, "decrypt", body, true).await?;
        remote::plaintext(KIND, &answer)
    }

    /// POST `body` to `{endpoint}/v1/{key}:{method}` with a fresh bearer
    /// token, map the status, and return the protected answer body.
    ///
    /// # Errors
    /// Returns [`KmsError::Authentication`] for a 400 on decrypt and
    /// [`KmsError::Unavailable`] otherwise.
    async fn call(
        &self,
        key: &str,
        method: &str,
        body: SecretBytes,
        decrypting: bool,
    ) -> Result<SecretBytes, KmsError> {
        let token = self
            .credentials
            .get_credential()
            .await
            .map_err(|_| unavailable(KIND, ProviderFailure::Credential))?;
        let request = self
            .client
            .post(format!("{}/v1/{key}:{method}", self.endpoint))
            .bearer_auth(&token.bearer)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(bytes::Bytes::from_owner(body));
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

/// A Cloud KMS encrypt or decrypt body borrowing its base64 fields, so
/// serializing it makes no unprotected copy of the key.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Request<'a> {
    /// Base64 plaintext DEK, on encrypt.
    #[serde(skip_serializing_if = "Option::is_none")]
    plaintext: Option<&'a str>,
    /// Base64 wrapped DEK, on decrypt.
    #[serde(skip_serializing_if = "Option::is_none")]
    ciphertext: Option<&'a str>,
    /// Base64 canonical wrap context.
    additional_authenticated_data: &'a str,
}
