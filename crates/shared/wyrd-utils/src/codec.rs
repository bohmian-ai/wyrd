//! Byte compression codecs shared by card implementations.

use std::io::{Cursor, Read, Write};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

use crate::WyrdUtilsResult;

/// Encode bytes with gzip.
///
/// # Errors
/// Returns an IO error when compression fails.
pub fn gzip_encode(bytes: &[u8]) -> WyrdUtilsResult<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}

/// Decode gzip-compressed bytes.
///
/// # Errors
/// Returns an IO error when decompression fails.
pub fn gzip_decode(bytes: &[u8]) -> WyrdUtilsResult<Vec<u8>> {
    let mut decoder = GzDecoder::new(Cursor::new(bytes));
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded)?;
    Ok(decoded)
}

/// Encode bytes with zstd.
///
/// # Errors
/// Returns an IO error when compression fails.
pub fn zstd_encode(bytes: &[u8]) -> WyrdUtilsResult<Vec<u8>> {
    Ok(zstd::stream::encode_all(Cursor::new(bytes), 0)?)
}

/// Decode zstd-compressed bytes.
///
/// # Errors
/// Returns an IO error when decompression fails.
pub fn zstd_decode(bytes: &[u8]) -> WyrdUtilsResult<Vec<u8>> {
    Ok(zstd::stream::decode_all(Cursor::new(bytes))?)
}

#[cfg(test)]
mod tests {
    use super::{gzip_decode, gzip_encode, zstd_decode, zstd_encode};

    #[test]
    fn gzip_round_trips_bytes() {
        let payload = b"{\"kind\":\"Data\"}\n";
        let encoded = gzip_encode(payload).expect("gzip encode should succeed");
        let decoded = gzip_decode(&encoded).expect("gzip decode should succeed");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn zstd_round_trips_bytes() {
        let payload = b"{\"kind\":\"Data\"}\n";
        let encoded = zstd_encode(payload).expect("zstd encode should succeed");
        let decoded = zstd_decode(&encoded).expect("zstd decode should succeed");
        assert_eq!(decoded, payload);
    }
}
