//! Boot-time storage preflight checks.

use crate::signer::BackendSigner;

/// Run non-fatal boot preflight checks for the selected backend.
///
/// Cloud-specific checks (e.g. the S3 incomplete-multipart lifecycle rule) live
/// in [`crate::cloud`] and run only when the crate is built with the `cloud`
/// feature. Local-only builds have no preflight work.
#[cfg(feature = "cloud")]
pub async fn run(signer: &BackendSigner) {
    if let BackendSigner::Cloud(cloud) = signer {
        cloud.preflight().await;
    }
}

/// Run the no-op preflight in a build without cloud support.
///
/// Local storage has no remote lifecycle checks. Returning an immediately
/// ready future keeps existing startup callers awaitable without introducing
/// an async state machine.
#[cfg(not(feature = "cloud"))]
pub fn run(_signer: &BackendSigner) -> std::future::Ready<()> {
    std::future::ready(())
}
