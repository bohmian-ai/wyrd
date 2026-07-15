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
    Io(#[from] std::io::Error),
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
pub(crate) async fn map_backend_response(response: reqwest::Response) -> StorageClientError {
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

impl From<StorageClientError> for WyrdError {
    fn from(error: StorageClientError) -> Self {
        match error {
            StorageClientError::Wyrd(err) => err,
            StorageClientError::SizeMismatch { expected, actual } => {
                WyrdStorageError::SizeMismatch { expected, actual }.into()
            }
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

#[cfg(test)]
mod tests {
    use super::{StorageClientError, map_backend_status};
    use wyrd_spec::error::{WyrdError, WyrdStorageError};

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

    #[test]
    fn transport_error_does_not_expose_reqwest_or_url_material() {
        let error = StorageClientError::Transport {
            operation: "upload",
        };
        let rendered = error.to_string();
        assert!(!rendered.contains("reqwest"));
        assert!(!rendered.contains("https://"));
    }

    #[test]
    fn presign_expiry_403_maps_to_presign_expired_catalog_entry() {
        let error: WyrdError = map_backend_status(403).into();
        assert_eq!(error.code(), "WYRD_STORAGE_503_PRESIGN_EXPIRED");
        assert_eq!(error.status(), 503);
    }

    #[test]
    fn precondition_412_maps_to_precondition_failed_catalog_entry() {
        let error: WyrdError = map_backend_status(412).into();
        assert_eq!(error.code(), "WYRD_STORAGE_412_PRECONDITION");
        assert_eq!(error.status(), 412);
    }

    #[test]
    fn range_416_maps_to_range_not_satisfiable_catalog_entry() {
        let error: WyrdError = map_backend_status(416).into();
        assert_eq!(error.code(), "WYRD_STORAGE_416_RANGE_NOT_SATISFIABLE");
        assert_eq!(error.status(), 416);
    }

    #[test]
    fn backend_5xx_maps_to_backend_unavailable_catalog_entry() {
        let error: WyrdError = map_backend_status(503).into();
        assert_eq!(error.code(), "WYRD_STORAGE_503_BACKEND_UNAVAILABLE");
        assert_eq!(error.status(), 503);
    }

    #[test]
    fn generic_4xx_maps_to_backend_catalog_entry() {
        let error: WyrdError = map_backend_status(400).into();
        assert_eq!(error.code(), "WYRD_STORAGE_500_BACKEND");
    }

    #[test]
    fn structured_wyrd_error_flows_through_from_conversion_unchanged() {
        let source: WyrdError = WyrdStorageError::PreconditionFailed.into();
        let boundary: WyrdError = StorageClientError::Wyrd(source.clone()).into();
        assert_eq!(source.code(), boundary.code());
        assert_eq!(source.status(), boundary.status());
    }
}
