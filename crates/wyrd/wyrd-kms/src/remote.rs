//! The bounded HTTP client and answer handling GCP and AWS KMS share.

use std::time::Duration;

use reqwest::{Client, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wyrd_crypt::SecretBytes;
use zeroize::Zeroizing;

use crate::{KmsBackendKind, KmsError, ProviderFailure};

/// Total time one wrap or unwrap call may take.
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// Time allowed to establish a provider connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest provider answer read; wrap and unwrap answers are tiny.
const MAX_ANSWER_BYTES: usize = 64 * 1024;

/// Build the reusable redirect-free client bounded by `timeout`.
///
/// # Errors
/// Returns [`KmsError::Config`] on `backend` when the client cannot be built.
pub(crate) fn client(timeout: Duration) -> Result<Client, KmsError> {
    wyrd_tls::install_crypto_provider().ok();
    Client::builder()
        .timeout(timeout)
        .connect_timeout(CONNECT_TIMEOUT.min(timeout))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| KmsError::Config {
            setting: "backend",
            reason: "could not build its HTTP client",
        })
}

/// The unavailable error of `backend` for `failure`.
pub(crate) const fn unavailable(backend: KmsBackendKind, failure: ProviderFailure) -> KmsError {
    KmsError::Unavailable { backend, failure }
}

/// Serialize a request `body` that may carry a key into protected bytes.
///
/// `body` must borrow its key fields so no unprotected copy exists.
///
/// # Errors
/// Returns [`KmsError::Encrypt`] when `body` cannot be serialized.
pub(crate) fn secret_json(body: &impl Serialize) -> Result<SecretBytes, KmsError> {
    let mut bytes = SecretBytes::default();
    serde_json::to_writer(&mut bytes, body).map_err(|_| KmsError::Encrypt)?;
    Ok(bytes)
}

/// Send `request` and return its status and bounded body.
///
/// The body stays in [`SecretBytes`] because a decrypt answer carries a
/// plaintext DEK.
///
/// # Errors
/// Returns [`ProviderFailure::Timeout`] or [`ProviderFailure::Transport`]
/// for a failed send or an oversized body.
pub(crate) async fn send(
    backend: KmsBackendKind,
    request: RequestBuilder,
) -> Result<(StatusCode, SecretBytes), KmsError> {
    let failed = |error: &reqwest::Error| {
        unavailable(
            backend,
            if error.is_timeout() {
                ProviderFailure::Timeout
            } else {
                ProviderFailure::Transport
            },
        )
    };
    let mut response = request.send().await.map_err(|error| failed(&error))?;
    let status = response.status();
    let mut body = SecretBytes::default();
    while let Some(chunk) = response.chunk().await.map_err(|error| failed(&error))? {
        if body.len() + chunk.len() > MAX_ANSWER_BYTES {
            return Err(unavailable(backend, ProviderFailure::Transport));
        }
        body.extend_from_slice(&chunk);
    }
    Ok((status, body))
}

/// Parse an answer that carries no key, such as an encrypt or error answer;
/// an empty body is `null`.
///
/// # Errors
/// Returns [`ProviderFailure::Transport`] for a body that is not JSON.
pub(crate) fn json(backend: KmsBackendKind, body: &[u8]) -> Result<Value, KmsError> {
    if body.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(body).map_err(|_| unavailable(backend, ProviderFailure::Transport))
}

/// A decrypt answer's base64 plaintext, borrowed from the protected body.
///
/// GCP names it `plaintext` and AWS `Plaintext`. Borrowing means an answer
/// that would need unescaping into a fresh string fails to parse and is
/// refused; provider base64 output never contains an escape.
#[derive(Deserialize)]
struct PlaintextAnswer<'a> {
    /// Base64 plaintext DEK.
    #[serde(alias = "Plaintext")]
    plaintext: &'a str,
}

/// Decode the plaintext DEK of a decrypt answer `body`.
///
/// # Errors
/// Returns [`ProviderFailure::Transport`] when the field is absent, escaped,
/// or not base64.
pub(crate) fn plaintext(backend: KmsBackendKind, body: &[u8]) -> Result<SecretBytes, KmsError> {
    use base64::Engine as _;
    serde_json::from_slice::<PlaintextAnswer<'_>>(body)
        .ok()
        .and_then(|answer| {
            base64::engine::general_purpose::STANDARD
                .decode(answer.plaintext)
                .ok()
        })
        .map(|dek| SecretBytes::from(Zeroizing::new(dek)))
        .ok_or(unavailable(backend, ProviderFailure::Transport))
}

/// Decode the base64 `field` of an answer that carries no key.
///
/// # Errors
/// Returns [`ProviderFailure::Transport`] when the field is absent or not
/// base64.
pub(crate) fn base64_field(
    backend: KmsBackendKind,
    body: &[u8],
    field: &str,
) -> Result<Vec<u8>, KmsError> {
    use base64::Engine as _;
    json(backend, body)?[field]
        .as_str()
        .and_then(|encoded| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
        })
        .ok_or(unavailable(backend, ProviderFailure::Transport))
}
