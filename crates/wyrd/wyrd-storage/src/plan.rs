//! Pure upload planner.

use wyrd_spec::storage::StorageBackendKind;

/// Default threshold where cloud backends switch to multipart upload.
pub const MULTIPART_THRESHOLD_BYTES: u64 = 100 * 1024 * 1024;
/// Default cloud multipart part size.
pub const DEFAULT_PART_SIZE_BYTES: u64 = 16 * 1024 * 1024;
/// Minimum part size accepted by S3 for non-final parts.
pub const MIN_PART_SIZE_BYTES: u64 = 5 * 1024 * 1024;
/// Maximum part size accepted by S3.
pub const MAX_PART_SIZE_BYTES: u64 = 5 * 1024 * 1024 * 1024;
/// Maximum part count accepted by S3.
pub const MAX_PARTS_PER_UPLOAD: u32 = 10_000;
/// Cross-backend maximum object size.
pub const MAX_OBJECT_SIZE_BYTES: u64 = 5 * 1024 * 1024 * 1024 * 1024;

/// Decided upload shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedUpload {
    /// Single PUT upload.
    SinglePut,
    /// Multipart upload.
    Multipart {
        /// Number of parts.
        part_count: u32,
        /// Size of each non-final part.
        part_size_bytes: u64,
    },
}

/// Planner errors.
#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    /// Artifact exceeds the cross-backend object cap.
    #[error("artifact size {0} exceeds cross-backend object size cap")]
    TooLarge(u64),
}

/// Plan an upload for a backend and object size.
///
/// # Errors
/// Returns [`PlanError::TooLarge`] when the object exceeds the cross-backend
/// maximum object size.
///
/// # Panics
/// Panics only if the internal planner invariant is broken and a computed part
/// count greater than [`MAX_PARTS_PER_UPLOAD`] reaches the final conversion.
pub fn plan_upload(
    size_bytes: u64,
    backend: StorageBackendKind,
) -> Result<PlannedUpload, PlanError> {
    if size_bytes > MAX_OBJECT_SIZE_BYTES {
        return Err(PlanError::TooLarge(size_bytes));
    }
    if backend == StorageBackendKind::Local || size_bytes < MULTIPART_THRESHOLD_BYTES {
        return Ok(PlannedUpload::SinglePut);
    }

    let mut part_size_bytes = DEFAULT_PART_SIZE_BYTES;
    let mut part_count = size_bytes.div_ceil(part_size_bytes);
    while part_count > u64::from(MAX_PARTS_PER_UPLOAD) {
        part_size_bytes = part_size_bytes.saturating_mul(2);
        if part_size_bytes > MAX_PART_SIZE_BYTES {
            return Err(PlanError::TooLarge(size_bytes));
        }
        part_count = size_bytes.div_ceil(part_size_bytes);
    }

    let part_count =
        u32::try_from(part_count).expect("invariant: part count <= MAX_PARTS_PER_UPLOAD");
    Ok(PlannedUpload::Multipart {
        part_count,
        part_size_bytes,
    })
}
