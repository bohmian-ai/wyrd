//! SHA-256 helpers.

use base64::Engine;
use sha2::{Digest, Sha256};

/// Hash in-memory bytes and return base64-standard SHA-256 output.
#[must_use]
pub fn bytes_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}
