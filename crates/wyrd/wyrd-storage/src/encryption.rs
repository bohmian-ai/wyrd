//! Server-side encryption header helpers.

use reqwest::header::HeaderMap;
use wyrd_spec::storage::StorageBackendKind;

/// AWS SSE header.
pub const AWS_SSE_HEADER: &str = "x-amz-server-side-encryption";
/// AWS SSE KMS key id header.
pub const AWS_SSE_KMS_KEY_ID_HEADER: &str = "x-amz-server-side-encryption-aws-kms-key-id";
/// GCS customer-managed key header.
pub const GCS_KMS_KEY_HEADER: &str = "x-goog-encryption-kms-key-name";
/// GCS encryption algorithm header.
pub const GCS_SSE_ALGORITHM_HEADER: &str = "x-goog-encryption-algorithm";
/// Azure server-encrypted header.
pub const AZURE_SERVER_ENCRYPTED_HEADER: &str = "x-ms-server-encrypted";
/// Azure encryption key SHA header.
pub const AZURE_ENCRYPTION_KEY_SHA_HEADER: &str = "x-ms-encryption-key-sha256";

/// Return the backend encryption marker from a HEAD response when present.
#[must_use]
pub fn encryption_marker(backend: StorageBackendKind, headers: &HeaderMap) -> Option<String> {
    match backend {
        StorageBackendKind::S3 => headers
            .get(AWS_SSE_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
        StorageBackendKind::Gcs => headers
            .get(GCS_SSE_ALGORITHM_HEADER)
            .or_else(|| headers.get(GCS_KMS_KEY_HEADER))
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
        StorageBackendKind::Azure => headers
            .get(AZURE_SERVER_ENCRYPTED_HEADER)
            .and_then(|value| value.to_str().ok())
            .filter(|value| value.eq_ignore_ascii_case("true"))
            .map(|_| "azure-default-encryption".to_owned()),
        StorageBackendKind::Local => Some("none".to_owned()),
    }
}

/// Decide whether a HEAD response advertises server-side encryption.
#[must_use]
pub fn is_encrypted(backend: StorageBackendKind, headers: &HeaderMap) -> bool {
    encryption_marker(backend, headers).is_some()
}
