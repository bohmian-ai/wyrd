//! AWS KMS wrapping: the `TrentService.Encrypt` and `TrentService.Decrypt`
//! JSON actions, signed with SigV4.
//!
//! The configured reference is a key ARN; its region selects the regional
//! endpoint unless `endpoint` overrides it. AWS retains historical key
//! material inside the logical key and selects it on Decrypt. The canonical
//! wrap context is sent as the encryption context `wyrd-context` (base64).
//! Credentials come from the standard AWS chain (environment, web identity,
//! container, or instance metadata), refreshed by `object_store`'s provider.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use object_store::aws::{AmazonS3Builder, AwsAuthorizer, AwsCredentialProvider};
use object_store::client::HttpRequestBody;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use url::Url;
use wyrd_crypt::SecretBytes;
use zeroize::Zeroizing;

use crate::config::aws_region;
use crate::remote::{self, unavailable};
use crate::{KmsBackendKind, KmsError, ProviderFailure};

/// This backend's kind.
const KIND: KmsBackendKind = KmsBackendKind::AwsKms;

/// A reusable authenticated AWS KMS client.
pub(crate) struct AwsKms {
    /// Bounded HTTP client.
    client: Client,
    /// Endpoint override for every region.
    endpoint: Option<String>,
    /// Workload credential chain.
    credentials: AwsCredentialProvider,
}

impl AwsKms {
    /// Build over the ambient AWS credential chain for keys in `region`.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] when the credential chain or client
    /// cannot be built.
    pub(crate) fn new(region: &str, endpoint: Option<&Url>) -> Result<Self, KmsError> {
        let store = AmazonS3Builder::from_env()
            .with_bucket_name("wyrd-kms")
            .with_region(region)
            .build()
            .map_err(|_| KmsError::Config {
                setting: "backend",
                reason: "could not load AWS workload credentials",
            })?;
        Self::with_credentials(
            endpoint.map(Url::as_str),
            store.credentials().clone(),
            remote::CALL_TIMEOUT,
        )
    }

    /// Build over explicit `credentials` and `timeout`.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] when the client cannot be built.
    pub(crate) fn with_credentials(
        endpoint: Option<&str>,
        credentials: AwsCredentialProvider,
        timeout: Duration,
    ) -> Result<Self, KmsError> {
        Ok(Self {
            client: remote::client(timeout)?,
            endpoint: endpoint.map(|endpoint| endpoint.trim_end_matches('/').to_owned()),
            credentials,
        })
    }

    /// Encrypt `dek` under key ARN `key`, authenticating `aad`.
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
            key_id: key,
            plaintext: Some(&plaintext),
            ciphertext_blob: None,
            encryption_context: Context {
                wyrd_context: &STANDARD.encode(aad),
            },
        })?;
        let answer = self.call(key, "TrentService.Encrypt", body).await?;
        remote::base64_field(KIND, &answer, "CiphertextBlob")
    }

    /// Decrypt `wrapped` under key ARN `key`, requiring `aad`.
    ///
    /// # Errors
    /// Returns [`KmsError::Authentication`] when KMS rejects the ciphertext,
    /// key, or context, and otherwise [`KmsError::Unavailable`].
    pub(crate) async fn unwrap(
        &self,
        key: &str,
        wrapped: &[u8],
        aad: &[u8],
    ) -> Result<SecretBytes, KmsError> {
        let body = remote::secret_json(&Request {
            key_id: key,
            plaintext: None,
            ciphertext_blob: Some(&STANDARD.encode(wrapped)),
            encryption_context: Context {
                wyrd_context: &STANDARD.encode(aad),
            },
        })?;
        let answer = self.call(key, "TrentService.Decrypt", body).await?;
        remote::plaintext(KIND, &answer)
    }

    /// Sign and POST one KMS JSON action for `key`, map its answer, and
    /// return the protected answer body.
    ///
    /// The signer and the request share one protected body.
    ///
    /// # Errors
    /// Returns [`KmsError::Authentication`] for `InvalidCiphertextException`
    /// or `IncorrectKeyException`, and [`KmsError::Unavailable`] otherwise.
    async fn call(
        &self,
        key: &str,
        target: &str,
        body: SecretBytes,
    ) -> Result<SecretBytes, KmsError> {
        let (partition, region) = aws_region(key).ok_or(KmsError::KeyNotAllowed)?;
        let url = self.endpoint.clone().unwrap_or_else(|| {
            let suffix = if partition == "aws-cn" {
                "amazonaws.com.cn"
            } else {
                "amazonaws.com"
            };
            format!("https://kms.{region}.{suffix}")
        });
        let credential = self
            .credentials
            .get_credential()
            .await
            .map_err(|_| unavailable(KIND, ProviderFailure::Credential))?;
        let bytes = bytes::Bytes::from_owner(body);
        let mut signed = http::Request::builder()
            .method("POST")
            .uri(format!("{url}/"))
            .header("content-type", "application/x-amz-json-1.1")
            .header("x-amz-target", target)
            .body(HttpRequestBody::from(bytes.clone()))
            .map_err(|_| unavailable(KIND, ProviderFailure::Transport))?;
        AwsAuthorizer::new(&credential, "kms", region)
            .try_authorize(&mut signed, None)
            .map_err(|_| unavailable(KIND, ProviderFailure::Credential))?;
        let request = self
            .client
            .post(format!("{url}/"))
            .headers(signed.headers().clone())
            .body(bytes);
        let (status, answer) = remote::send(KIND, request).await?;
        if status == StatusCode::OK {
            return Ok(answer);
        }
        let answer = remote::json(KIND, &answer)?;
        let kind = answer["__type"].as_str().unwrap_or_default();
        if kind.ends_with("InvalidCiphertextException") || kind.ends_with("IncorrectKeyException") {
            return Err(KmsError::Authentication);
        }
        Err(unavailable(
            KIND,
            if status.is_client_error() {
                ProviderFailure::Refused
            } else {
                ProviderFailure::Transport
            },
        ))
    }
}

/// A KMS Encrypt or Decrypt body borrowing its base64 fields, so
/// serializing it makes no unprotected copy of the key.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Request<'a> {
    /// The configured key ARN.
    key_id: &'a str,
    /// Base64 plaintext DEK, on Encrypt.
    #[serde(skip_serializing_if = "Option::is_none")]
    plaintext: Option<&'a str>,
    /// Base64 wrapped DEK, on Decrypt.
    #[serde(skip_serializing_if = "Option::is_none")]
    ciphertext_blob: Option<&'a str>,
    /// The encryption context.
    encryption_context: Context<'a>,
}

/// The encryption context: the base64 canonical wrap context under
/// `wyrd-context`.
#[derive(Serialize)]
struct Context<'a> {
    /// Base64 canonical wrap context.
    #[serde(rename = "wyrd-context")]
    wyrd_context: &'a str,
}
