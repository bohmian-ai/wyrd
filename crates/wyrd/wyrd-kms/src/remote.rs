//! The bounded HTTP client and answer handling GCP and AWS KMS share.

use std::time::Duration;

use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::Value;
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

/// Send `request` and return its status and bounded JSON answer.
///
/// The body is zeroized after parsing because a decrypt answer carries a
/// plaintext DEK.
///
/// # Errors
/// Returns [`ProviderFailure::Timeout`] or [`ProviderFailure::Transport`]
/// for a failed send, an oversized body, or a body that is not JSON.
pub(crate) async fn send(
    backend: KmsBackendKind,
    request: RequestBuilder,
) -> Result<(StatusCode, Value), KmsError> {
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
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = response.chunk().await.map_err(|error| failed(&error))? {
        if body.len() + chunk.len() > MAX_ANSWER_BYTES {
            return Err(unavailable(backend, ProviderFailure::Transport));
        }
        body.extend_from_slice(&chunk);
    }
    let answer = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| unavailable(backend, ProviderFailure::Transport))?
    };
    Ok((status, answer))
}

/// Decode a base64 field of a provider answer.
///
/// # Errors
/// Returns [`ProviderFailure::Transport`] when the field is absent or not
/// base64.
pub(crate) fn base64_field(
    backend: KmsBackendKind,
    answer: &Value,
    field: &str,
) -> Result<Zeroizing<Vec<u8>>, KmsError> {
    use base64::Engine as _;
    answer[field]
        .as_str()
        .and_then(|encoded| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
        })
        .map(Zeroizing::new)
        .ok_or(unavailable(backend, ProviderFailure::Transport))
}
