//! The common stored-secret envelope encoding.
//!
//! ```text
//! "WKMS" | format 0x01 | algorithm 0x01 | ref_len u16 BE | key_ref
//!        | wrapped_len u32 BE | wrapped_dek | nonce (12) | ciphertext
//! ```
//!
//! Format 1 with algorithm 1 is AES-256-GCM under a 32-byte DEK. The wrapped
//! DEK is the backend's opaque blob; Wyrd never parses a provider's format.

use crate::KmsError;

/// Magic prefix of every envelope.
const MAGIC: &[u8; 4] = b"WKMS";
/// The only envelope format this build reads and writes.
const FORMAT_V1: u8 = 1;
/// AES-256-GCM payload under a 32-byte DEK.
const ALG_AES_256_GCM: u8 = 1;
/// AES-GCM nonce length.
const NONCE_LEN: usize = 12;
/// AES-GCM tag length; every ciphertext is at least this long.
const TAG_LEN: usize = 16;

/// One decoded envelope borrowing the stored bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Envelope<'a> {
    /// Logical wrapping-key reference.
    pub(crate) key_ref: &'a str,
    /// Backend-wrapped DEK.
    pub(crate) wrapped_dek: &'a [u8],
    /// Payload nonce.
    pub(crate) nonce: [u8; NONCE_LEN],
    /// Authenticated payload ciphertext.
    pub(crate) ciphertext: &'a [u8],
}

impl<'a> Envelope<'a> {
    /// Encode this envelope into its stored bytes.
    ///
    /// # Errors
    /// Returns [`KmsError::Encrypt`] when the reference or wrapped DEK
    /// exceeds its length field.
    pub(crate) fn encode(&self) -> Result<Vec<u8>, KmsError> {
        let ref_len = u16::try_from(self.key_ref.len()).map_err(|_| KmsError::Encrypt)?;
        let wrapped_len = u32::try_from(self.wrapped_dek.len()).map_err(|_| KmsError::Encrypt)?;
        let mut out = Vec::with_capacity(
            MAGIC.len()
                + 2
                + 2
                + self.key_ref.len()
                + 4
                + self.wrapped_dek.len()
                + NONCE_LEN
                + self.ciphertext.len(),
        );
        out.extend_from_slice(MAGIC);
        out.push(FORMAT_V1);
        out.push(ALG_AES_256_GCM);
        out.extend_from_slice(&ref_len.to_be_bytes());
        out.extend_from_slice(self.key_ref.as_bytes());
        out.extend_from_slice(&wrapped_len.to_be_bytes());
        out.extend_from_slice(self.wrapped_dek);
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(self.ciphertext);
        Ok(out)
    }

    /// Decode stored bytes.
    ///
    /// # Errors
    /// Returns [`KmsError::UnknownFormat`] for any other magic, format, or
    /// algorithm, a truncated field, a non-UTF-8 reference, or a ciphertext
    /// shorter than its tag.
    pub(crate) fn decode(bytes: &'a [u8]) -> Result<Self, KmsError> {
        let rest = bytes
            .strip_prefix(MAGIC.as_slice())
            .ok_or(KmsError::UnknownFormat)?;
        let (&[format, algorithm], rest) = split(rest, 2)? else {
            return Err(KmsError::UnknownFormat);
        };
        if format != FORMAT_V1 || algorithm != ALG_AES_256_GCM {
            return Err(KmsError::UnknownFormat);
        }
        let (ref_len, rest) = split(rest, 2)?;
        let ref_len = usize::from(u16::from_be_bytes([ref_len[0], ref_len[1]]));
        let (key_ref, rest) = split(rest, ref_len)?;
        let key_ref = std::str::from_utf8(key_ref).map_err(|_| KmsError::UnknownFormat)?;
        let (wrapped_len, rest) = split(rest, 4)?;
        let wrapped_len = u32::from_be_bytes([
            wrapped_len[0],
            wrapped_len[1],
            wrapped_len[2],
            wrapped_len[3],
        ]);
        let wrapped_len = usize::try_from(wrapped_len).map_err(|_| KmsError::UnknownFormat)?;
        let (wrapped_dek, rest) = split(rest, wrapped_len)?;
        let (nonce, ciphertext) = split(rest, NONCE_LEN)?;
        if ciphertext.len() < TAG_LEN {
            return Err(KmsError::UnknownFormat);
        }
        let mut fixed = [0_u8; NONCE_LEN];
        fixed.copy_from_slice(nonce);
        Ok(Self {
            key_ref,
            wrapped_dek,
            nonce: fixed,
            ciphertext,
        })
    }
}

/// Split `len` bytes off the front of `bytes`.
///
/// # Errors
/// Returns [`KmsError::UnknownFormat`] when fewer than `len` bytes remain.
fn split(bytes: &[u8], len: usize) -> Result<(&[u8], &[u8]), KmsError> {
    bytes.split_at_checked(len).ok_or(KmsError::UnknownFormat)
}
