//! Streaming SHA-256 helpers.

use base64::Engine;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt};

/// Chunk size for streaming hashes.
pub const CHUNK_BYTES: usize = 64 * 1024;

/// Stream SHA-256 over an async reader and return base64-standard output.
///
/// # Errors
/// Returns the reader's IO error if reading fails.
pub async fn stream_sha256<R>(reader: &mut R) -> Result<String, std::io::Error>
where
    R: AsyncRead + Unpin,
{
    let mut hasher = Sha256::new();
    let mut buf = vec![0_u8; CHUNK_BYTES];
    loop {
        let read = reader.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(hasher.finalize()))
}

/// Hash in-memory bytes and return base64-standard SHA-256 output.
#[must_use]
pub fn bytes_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}
