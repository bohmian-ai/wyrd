//! Storage client errors.
//!
//! Boundary errors preserve their stable Wyrd identity so callers can make
//! machine-readable retry, refresh, and cleanup decisions. The client never
//! surfaces `reqwest::Error`, raw URLs, or bearer tokens through the boundary.
//! Structured backend failures — expired presigned URLs, precondition
//! mismatches, backend unavailability — flow through
//! [`StorageClientError::Wyrd`] carrying the exact catalog variant, and the
//! `From<StorageClientError> for WyrdError` conversion routes those through
//! unchanged (V-002).

use reqwest::Response;
use std::io::Error as IoError;
use thiserror::Error;
use wyrd_spec::error::{WyrdError, WyrdStorageError};

/// Errors returned by the client-side artifact transfer driver.
#[derive(Debug, Error)]
pub enum StorageClientError {
    /// The upload plan did not match the expected protocol data.
    #[error("upload plan mismatch: expected {expected}, got {actual}")]
    PlanMismatch {
        /// Expected plan variant.
        expected: &'static str,
        /// Actual plan variant.
        actual: &'static str,
    },
    /// A required upload hook was not supplied.
    #[error("upload hook is required: {0}")]
    MissingHook(&'static str),
    /// The handle was not configured with a capability required by the plan.
    #[error("storage capability is required: {0}")]
    MissingCapability(&'static str),
    /// The server-supplied plan contains unusable dimensions.
    #[error("invalid upload plan: {0}")]
    PlanInvalid(&'static str),
    /// The source could not be read.
    #[error("artifact source failed: {0}")]
    Source(String),
    /// Local destination or source IO failed.
    #[error("artifact filesystem operation failed: {0}")]
    Io(#[from] IoError),
    /// The backend request could not be sent.
    #[error("artifact transfer transport failed during {operation}")]
    Transport {
        /// Operation being attempted.
        operation: &'static str,
    },
    /// A multipart response omitted the required ETag.
    #[error("S3 part {part_number} response did not include an ETag")]
    MissingETag {
        /// One-based part number.
        part_number: u32,
    },
    /// The plan and source lengths disagree.
    #[error("artifact size mismatch: expected {expected}, got {actual}")]
    SizeMismatch {
        /// Expected size or count.
        expected: u64,
        /// Actual size or count.
        actual: u64,
    },
    /// The downloaded bytes did not match the server-declared digest or size.
    #[error(
        "artifact verification failed: expected {expected_sha256} / {expected_size} bytes, got {actual_sha256} / {actual_size} bytes"
    )]
    VerifyFailed {
        /// Expected base64-encoded SHA-256 digest.
        expected_sha256: String,
        /// Computed base64-encoded SHA-256 digest.
        actual_sha256: String,
        /// Expected byte length.
        expected_size: u64,
        /// Downloaded byte length.
        actual_size: u64,
    },
    /// A structured Wyrd error crossed the boundary; surfaced verbatim so
    /// stable `code`, `status`, `remediation`, and `details` are preserved.
    #[error("{0}")]
    Wyrd(WyrdError),
}

/// Backend HTTP-status mapper for external (S3/GCS/Azure) responses.
///
/// Consumes the response, discarding the URL and any bearer material, and
/// projects the backend status onto the stable [`WyrdStorageError`] catalog.
/// Called by every external upload/download branch on the non-success path so
/// callers see the same stable codes regardless of protocol (V-002).
pub(crate) async fn map_backend_response(response: Response) -> StorageClientError {
    let status = response.status().as_u16();
    let _ = response.bytes().await;
    map_backend_status(status)
}

/// Map a backend HTTP status to a stable [`WyrdStorageError`] variant.
pub(crate) fn map_backend_status(status: u16) -> StorageClientError {
    let err = match status {
        403 => WyrdStorageError::PresignExpired {
            detail: format!("backend rejected presigned URL with HTTP {status}"),
        },
        412 => WyrdStorageError::PreconditionFailed,
        416 => WyrdStorageError::RangeNotSatisfiable,
        s if (500..600).contains(&s) => WyrdStorageError::BackendUnavailable { status: s },
        s => WyrdStorageError::Backend {
            detail: format!("backend returned HTTP {s}"),
        },
    };
    StorageClientError::Wyrd(err.into())
}

/// Wrap an authenticated-transport [`WyrdError`] as a boundary storage error
/// without losing its structured identity.
pub(crate) fn from_authenticated(err: WyrdError) -> StorageClientError {
    StorageClientError::Wyrd(err)
}

/// Project a storage client failure onto the public storage catalog.
///
/// Structured [`WyrdError`]s pass through unchanged; local failures map to their
/// `WYRD_STORAGE_*` entries without exposing transport or URL material.
impl From<StorageClientError> for WyrdError {
    /// Map each local variant to its catalog entry, passing structured errors through.
    fn from(error: StorageClientError) -> Self {
        match error {
            StorageClientError::Wyrd(err) => err,
            StorageClientError::SizeMismatch { expected, actual } => {
                WyrdStorageError::SizeMismatch { expected, actual }.into()
            }
            StorageClientError::VerifyFailed {
                expected_sha256,
                actual_sha256,
                expected_size,
                actual_size,
            } => WyrdError::RegistryArtifactVerifyFailed {
                message: "downloaded artifact did not match the server declaration".to_owned(),
                details: serde_json::json!({
                    "expected_sha256": expected_sha256,
                    "actual_sha256": actual_sha256,
                    "expected_size": expected_size,
                    "actual_size": actual_size,
                }),
            },
            StorageClientError::PlanMismatch { expected, actual } => WyrdStorageError::Backend {
                detail: format!("invalid upload plan: expected {expected}, got {actual}"),
            }
            .into(),
            StorageClientError::MissingHook(hook) => WyrdStorageError::Backend {
                detail: format!("required client hook is missing: {hook}"),
            }
            .into(),
            StorageClientError::MissingCapability(capability) => WyrdStorageError::Backend {
                detail: format!("required storage capability is missing: {capability}"),
            }
            .into(),
            StorageClientError::PlanInvalid(detail) => WyrdStorageError::Backend {
                detail: detail.to_owned(),
            }
            .into(),
            StorageClientError::Source(detail) => WyrdStorageError::Backend { detail }.into(),
            StorageClientError::Io(_) | StorageClientError::Transport { .. } => {
                WyrdStorageError::Backend {
                    detail: "artifact transfer failed".to_owned(),
                }
                .into()
            }
            StorageClientError::MissingETag { part_number } => WyrdStorageError::Backend {
                detail: format!("S3 part {part_number} response omitted ETag"),
            }
            .into(),
        }
    }
}

/// Storage error to catalog projection.
#[cfg(test)]
mod tests {
    use super::{StorageClientError, map_backend_status};
    use wyrd_spec::error::{WyrdError, WyrdStorageError};

    /// Size mismatches map to `WYRD_STORAGE_400_SIZE_MISMATCH`.
    #[test]
    fn size_mismatch_maps_to_storage_catalog() {
        let error: WyrdError = StorageClientError::SizeMismatch {
            expected: 4,
            actual: 3,
        }
        .into();
        assert_eq!(error.code(), "WYRD_STORAGE_400_SIZE_MISMATCH");
        assert_eq!(error.status(), 400);
    }

    /// Transport failures render without library names or URLs.
    #[test]
    fn transport_error_does_not_expose_reqwest_or_url_material() {
        let error = StorageClientError::Transport {
            operation: "upload",
        };
        let rendered = error.to_string();
        assert!(!rendered.contains("reqwest"));
        assert!(!rendered.contains("https://"));
    }

    /// A backend 403 maps to the presign-expired catalog entry.
    #[test]
    fn presign_expiry_403_maps_to_presign_expired_catalog_entry() {
        let error: WyrdError = map_backend_status(403).into();
        assert_eq!(error.code(), "WYRD_STORAGE_503_PRESIGN_EXPIRED");
        assert_eq!(error.status(), 503);
    }

    /// A backend 412 maps to the precondition-failed catalog entry.
    #[test]
    fn precondition_412_maps_to_precondition_failed_catalog_entry() {
        let error: WyrdError = map_backend_status(412).into();
        assert_eq!(error.code(), "WYRD_STORAGE_412_PRECONDITION");
        assert_eq!(error.status(), 412);
    }

    /// A backend 416 maps to the range-not-satisfiable catalog entry.
    #[test]
    fn range_416_maps_to_range_not_satisfiable_catalog_entry() {
        let error: WyrdError = map_backend_status(416).into();
        assert_eq!(error.code(), "WYRD_STORAGE_416_RANGE_NOT_SATISFIABLE");
        assert_eq!(error.status(), 416);
    }

    /// A backend 5xx maps to the backend-unavailable catalog entry.
    #[test]
    fn backend_5xx_maps_to_backend_unavailable_catalog_entry() {
        let error: WyrdError = map_backend_status(503).into();
        assert_eq!(error.code(), "WYRD_STORAGE_503_BACKEND_UNAVAILABLE");
        assert_eq!(error.status(), 503);
    }

    /// Any other backend 4xx maps to the generic backend catalog entry.
    #[test]
    fn generic_4xx_maps_to_backend_catalog_entry() {
        let error: WyrdError = map_backend_status(400).into();
        assert_eq!(error.code(), "WYRD_STORAGE_500_BACKEND");
    }

    /// A structured Wyrd error keeps its code and status through the conversion.
    #[test]
    fn structured_wyrd_error_flows_through_from_conversion_unchanged() {
        let source: WyrdError = WyrdStorageError::PreconditionFailed.into();
        let boundary: WyrdError = StorageClientError::Wyrd(source.clone()).into();
        assert_eq!(source.code(), boundary.code());
        assert_eq!(source.status(), boundary.status());
    }
}
