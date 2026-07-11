//! Boot-time storage preflight checks.

use crate::signer::BackendSigner;

/// Run non-fatal boot preflight checks for the selected backend.
///
/// Cloud-specific checks (e.g. the S3 incomplete-multipart lifecycle rule) live
/// in [`crate::cloud`] and run only when the crate is built with the `cloud`
/// feature. Local-only builds have no preflight work.
pub async fn run(signer: &BackendSigner) {
    #[cfg(feature = "cloud")]
    if let BackendSigner::Cloud(cloud) = signer {
        cloud.preflight().await;
    }
}
